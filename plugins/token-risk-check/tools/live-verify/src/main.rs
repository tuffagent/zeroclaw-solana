// Loads the compiled token_risk_check.wasm via WasmTool::from_wasm, the
// same host-side path zeroclaw itself uses at plugin registration and
// call time, then executes it against a real mint over a real RPC. This
// exists because zeroclaw has no CLI subcommand for invoking a tool
// directly - the only sanctioned path is a full agent conversation loop
// deciding, via its own LLM, to call the tool - which is unsuited to a
// deterministic CI verification pass.

use std::collections::HashMap;
use std::path::PathBuf;

use zeroclaw_api::tool::Tool;
use zeroclaw_plugins::PluginPermission;
use zeroclaw_plugins::component::PluginLimits;
use zeroclaw_plugins::wasm_tool::WasmTool;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mint = std::env::args()
        .nth(1)
        .expect("usage: live-verify <mint-address>");

    let mut config: HashMap<String, String> = HashMap::new();
    config.insert(
        "rpc_url".to_string(),
        std::env::var("TRC_RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string()),
    );
    config.insert(
        "ollama_enabled".to_string(),
        std::env::var("TRC_OLLAMA_ENABLED").unwrap_or_else(|_| "false".to_string()),
    );
    if let Ok(v) = std::env::var("TRC_OLLAMA_ENDPOINT") {
        config.insert("ollama_endpoint".to_string(), v);
    }
    if let Ok(v) = std::env::var("TRC_OLLAMA_MODEL") {
        config.insert("ollama_model".to_string(), v);
    }

    let wasm_path = PathBuf::from(std::env::var("TRC_WASM_PATH").unwrap_or_else(|_| {
        "target/wasm32-wasip2/release/token_risk_check.wasm".to_string()
    }));

    // Matches manifest.toml's declared permissions exactly.
    let permissions = vec![PluginPermission::ConfigRead, PluginPermission::HttpClient];

    // Production defaults from PluginLimitsConfig (zeroclaw-config schema.rs).
    let limits = PluginLimits {
        call_fuel: 1_000_000_000,
        max_memory_bytes: 256 * 1024 * 1024,
        max_table_elements: 100_000,
        max_instances: 64,
    };

    let tool = WasmTool::from_wasm(
        wasm_path,
        permissions,
        "token-risk-check".to_string(),
        "Rug/custody risk assessment for a Solana SPL Token or Token-2022 mint".to_string(),
        config,
        limits,
    );

    // from_wasm probes the component's real tool export for its name and
    // description, so this confirms registration succeeded (not a
    // fallback schema) before the actual call.
    eprintln!("registered as: {} ({})", tool.name(), tool.description());

    let result = tool.execute(serde_json::json!({ "mint": mint })).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}
