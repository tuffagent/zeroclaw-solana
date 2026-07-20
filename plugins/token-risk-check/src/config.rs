use std::collections::HashMap;

// Config-section parsing, mirroring the jail contract every ZeroClaw
// plugin follows: the host hands over a flat `string -> string` map, so
// every typed field is a parse-with-default, and an empty map must
// produce safe behaviour (exactly what a plugin without `config_read`, or
// an unconfigured one, receives).

#[derive(Debug, Clone)]
pub struct RiskCheckConfig {
    pub rpc_url: String,
    pub amber_threshold: f64,
    pub red_threshold: f64,
    pub check_liquidity: bool,
    pub ollama_enabled: bool,
    pub ollama_endpoint: String,
    pub ollama_model: String,
}

impl RiskCheckConfig {
    pub fn from_section(section: &HashMap<String, String>) -> Self {
        Self {
            rpc_url: string_or(section, "rpc_url", "https://api.mainnet-beta.solana.com"),
            amber_threshold: number_or(section, "amber_threshold", 25.0),
            red_threshold: number_or(section, "red_threshold", 60.0),
            check_liquidity: bool_or(section, "check_liquidity", true),
            ollama_enabled: bool_or(section, "ollama_enabled", false),
            ollama_endpoint: string_or(section, "ollama_endpoint", "http://localhost:11434"),
            ollama_model: string_or(section, "ollama_model", "qwen2.5:0.5b"),
        }
    }
}

fn string_or(section: &HashMap<String, String>, key: &str, default: &str) -> String {
    section
        .get(key)
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn number_or(section: &HashMap<String, String>, key: &str, default: f64) -> f64 {
    section.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn bool_or(section: &HashMap<String, String>, key: &str, default: bool) -> bool {
    section
        .get(key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_section_produces_documented_defaults() {
        let cfg = RiskCheckConfig::from_section(&HashMap::new());
        assert_eq!(cfg.rpc_url, "https://api.mainnet-beta.solana.com");
        assert_eq!(cfg.amber_threshold, 25.0);
        assert_eq!(cfg.red_threshold, 60.0);
        assert!(cfg.check_liquidity);
        assert!(!cfg.ollama_enabled);
        assert_eq!(cfg.ollama_endpoint, "http://localhost:11434");
        assert_eq!(cfg.ollama_model, "qwen2.5:0.5b");
    }

    #[test]
    fn explicit_values_override_defaults() {
        let mut section = HashMap::new();
        section.insert("rpc_url".to_string(), "https://my-rpc.example".to_string());
        section.insert("amber_threshold".to_string(), "10".to_string());
        section.insert("ollama_enabled".to_string(), "true".to_string());
        section.insert("check_liquidity".to_string(), "false".to_string());
        let cfg = RiskCheckConfig::from_section(&section);
        assert_eq!(cfg.rpc_url, "https://my-rpc.example");
        assert_eq!(cfg.amber_threshold, 10.0);
        assert!(cfg.ollama_enabled);
        assert!(!cfg.check_liquidity);
    }

    #[test]
    fn malformed_numeric_value_falls_back_to_default_rather_than_panicking() {
        let mut section = HashMap::new();
        section.insert("red_threshold".to_string(), "not-a-number".to_string());
        let cfg = RiskCheckConfig::from_section(&section);
        assert_eq!(cfg.red_threshold, 60.0);
    }
}
