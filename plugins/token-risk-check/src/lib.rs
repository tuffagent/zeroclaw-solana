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

    use exports::zeroclaw::plugin::plugin_info::Guest as PluginInfo;
    use exports::zeroclaw::plugin::tool::{Guest as Tool, ToolResult};

    struct TokenRiskCheck;

    impl PluginInfo for TokenRiskCheck {
        fn plugin_name() -> String {
            "token-risk-check".to_string()
        }
        fn plugin_version() -> String {
            env!("CARGO_PKG_VERSION").to_string()
        }
    }

    impl Tool for TokenRiskCheck {
        fn name() -> String {
            "token_risk_check".to_string()
        }
        fn description() -> String {
            "stub, filled in by a later task".to_string()
        }
        fn parameters_schema() -> String {
            "{}".to_string()
        }
        fn execute(_args: String) -> Result<ToolResult, String> {
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("not implemented yet".to_string()),
            })
        }
    }

    export!(TokenRiskCheck);
}
