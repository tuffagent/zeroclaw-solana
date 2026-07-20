//! Pure parsing for the optional Jupiter token-list liquidity check. The
//! actual HTTP GET lives in the wasm shim (Task 11); this module only
//! decides, from an already-fetched JSON body, whether a mint appears —
//! informational only, never fed into the risk score.

pub fn mint_is_listed(list_json: &str, mint: &str) -> Result<bool, String> {
    let value: serde_json::Value =
        serde_json::from_str(list_json).map_err(|e| format!("invalid JSON: {e}"))?;
    let entries = value
        .as_array()
        .ok_or_else(|| "expected a JSON array of token entries".to_string())?;
    Ok(entries.iter().any(|entry| {
        entry
            .get("address")
            .and_then(|a| a.as_str())
            .map(|a| a == mint)
            .unwrap_or(false)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LIST: &str = r#"[{"address":"MintA111"},{"address":"MintB222"}]"#;

    #[test]
    fn finds_a_listed_mint() {
        assert!(mint_is_listed(SAMPLE_LIST, "MintA111").unwrap());
    }

    #[test]
    fn does_not_find_an_unlisted_mint() {
        assert!(!mint_is_listed(SAMPLE_LIST, "MintZ999").unwrap());
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(mint_is_listed("not json", "MintA111").is_err());
    }

    #[test]
    fn non_array_json_is_an_error() {
        assert!(mint_is_listed(r#"{"not":"an array"}"#, "MintA111").is_err());
    }
}
