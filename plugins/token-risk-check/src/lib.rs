pub mod config;
pub mod core_logic;
pub mod liquidity;
pub mod narrate;

#[cfg(target_family = "wasm")]
mod component {
    wit_bindgen::generate!({
        path: "../../wit/v0",
        world: "tool-plugin",
        features: ["plugins-wit-v0"],
    });

    use std::collections::HashMap;

    use exports::zeroclaw::plugin::plugin_info::Guest as PluginInfo;
    use exports::zeroclaw::plugin::tool::{Guest as Tool, ToolResult};
    use zeroclaw::plugin::logging::{
        log_record, LogLevel, PluginAction, PluginEvent, PluginOutcome,
    };

    use solana_core::transport::RpcTransport;

    use crate::config::RiskCheckConfig;
    use crate::core_logic;
    use crate::liquidity;
    use crate::narrate;

    const PLUGIN_NAME: &str = "token-risk-check";
    const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
    const TOOL_NAME: &str = "token_risk_check";

    struct WakiTransport;

    impl RpcTransport for WakiTransport {
        fn post(&self, url: &str, body: &str) -> Result<String, String> {
            let resp = waki::Client::new()
                .post(url)
                .header("Content-Type", "application/json")
                .body(body.as_bytes().to_vec())
                .connect_timeout(std::time::Duration::from_secs(10))
                .send()
                .map_err(|e| format!("RPC request failed: {e}"))?;
            let bytes = resp
                .body()
                .map_err(|e| format!("failed to read RPC response body: {e}"))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }

    fn fetch_liquidity(config: &RiskCheckConfig, mint: &str) -> &'static str {
        if !config.check_liquidity {
            return "unknown";
        }
        let result = waki::Client::new()
            .get("https://lite-api.jup.ag/tokens/v1/mints/tradable")
            .connect_timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| e.to_string())
            .and_then(|resp| resp.body().map_err(|e| e.to_string()))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .and_then(|text| liquidity::mint_is_listed(&text, mint));
        match result {
            Ok(true) => "found",
            Ok(false) => "not_found",
            Err(_) => "unknown",
        }
    }

    fn fetch_summary(config: &RiskCheckConfig, score: f64, verdict: &str, factors_json: &str) -> String {
        if !config.ollama_enabled {
            return narrate::fallback_summary(score, verdict);
        }
        let prompt = narrate::build_prompt(score, verdict, factors_json);
        let body = narrate::build_request_body(&config.ollama_model, &prompt);
        let url = format!("{}/api/generate", config.ollama_endpoint.trim_end_matches('/'));
        let result = waki::Client::new()
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.into_bytes())
            .connect_timeout(std::time::Duration::from_secs(5))
            .send()
            .map_err(|e| e.to_string())
            .and_then(|resp| resp.body().map_err(|e| e.to_string()))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .and_then(|raw| narrate::parse_response(&raw));
        result.unwrap_or_else(|_| narrate::fallback_summary(score, verdict))
    }

    struct TokenRiskCheck;

    #[derive(serde::Deserialize)]
    struct ExecuteArgs {
        mint: String,
        #[serde(rename = "__config", default)]
        config: HashMap<String, String>,
    }

    impl PluginInfo for TokenRiskCheck {
        fn plugin_name() -> String {
            PLUGIN_NAME.to_string()
        }
        fn plugin_version() -> String {
            PLUGIN_VERSION.to_string()
        }
    }

    impl Tool for TokenRiskCheck {
        fn name() -> String {
            TOOL_NAME.to_string()
        }

        fn description() -> String {
            "Assess the rug/custody risk of a Solana SPL Token or Token-2022 mint: mint/freeze \
             authority, dangerous Token-2022 extensions (permanent delegate, transfer hook, \
             transfer fee, default-frozen accounts), and top-holder concentration. Read-only; \
             never signs or moves funds. Returns a 0-100 score and a green/amber/red verdict \
             biased toward flagging when uncertain."
                .to_string()
        }

        fn parameters_schema() -> String {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "mint": {
                        "type": "string",
                        "description": "Base58-encoded Solana mint address to assess."
                    }
                },
                "required": ["mint"]
            })
            .to_string()
        }

        fn execute(args: String) -> Result<ToolResult, String> {
            let parsed: ExecuteArgs = match serde_json::from_str(&args) {
                Ok(a) => a,
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("invalid arguments: {e}")),
                    });
                }
            };

            let config = RiskCheckConfig::from_section(&parsed.config);
            let transport = WakiTransport;

            let result = match core_logic::run(&transport, &config, &parsed.mint) {
                Ok(r) => r,
                Err(e) => {
                    emit(PluginAction::Fail, PluginOutcome::Failure, "risk check failed");
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(e),
                    });
                }
            };

            let factors_value = serde_json::json!({
                "mint_authority": result.mint_authority_present,
                "freeze_authority": result.freeze_authority_present,
                "extensions": {
                    "permanent_delegate": result.extensions.permanent_delegate,
                    "transfer_hook": result.extensions.transfer_hook,
                    "transfer_fee_config": result.extensions.transfer_fee_config,
                    "default_account_state_frozen": result.extensions.default_account_state_frozen,
                    "non_transferable": result.extensions.non_transferable,
                    "confidential_transfer": result.extensions.confidential_transfer,
                },
                // Null, never zeroes: a missing holder reading and a mint
                // whose top holders genuinely hold nothing must not arrive
                // at a caller looking identical.
                "concentration": result.concentration.map(|c| serde_json::json!({
                    "top1_pct": c.top1_pct,
                    "top5_pct": c.top5_pct,
                    "top10_pct": c.top10_pct,
                    "top20_pct": c.top20_pct,
                })),
            });
            let factors_json = factors_value.to_string();

            let liquidity = fetch_liquidity(&config, &parsed.mint);
            let verdict = result.verdict.as_str();
            let summary = fetch_summary(&config, result.score, verdict, &factors_json);

            let output = serde_json::json!({
                "score": result.score,
                "verdict": verdict,
                "factors": factors_value,
                "liquidity": liquidity,
                "context": {
                    "decimals": result.decimals,
                    "total_supply": result.supply.to_string(),
                },
                "warnings": result.warnings,
                "summary": summary,
            })
            .to_string();

            emit(PluginAction::Complete, PluginOutcome::Success, "risk check complete");

            Ok(ToolResult {
                success: true,
                output,
                error: None,
            })
        }
    }

    fn emit(action: PluginAction, outcome: PluginOutcome, message: &str) {
        log_record(
            LogLevel::Info,
            &PluginEvent {
                function_name: "token_risk_check::tool::execute".to_string(),
                action,
                outcome: Some(outcome),
                duration_ms: None,
                attrs: None,
                message: message.to_string(),
            },
        );
    }

    export!(TokenRiskCheck);
}
