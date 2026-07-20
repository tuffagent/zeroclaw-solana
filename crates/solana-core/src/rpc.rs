//! JSON-RPC request construction and response parsing for the two Solana
//! RPC methods this crate needs. Not a general-purpose client — only what
//! `token-risk-check` (and any future importer) actually calls.

use crate::transport::RpcTransport;

#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub owner: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct LargestAccount {
    pub amount: u64,
}

pub fn get_account_info(
    transport: &dyn RpcTransport,
    rpc_url: &str,
    pubkey: &str,
) -> Result<Option<AccountInfo>, String> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":["{pubkey}",{{"encoding":"base64"}}]}}"#
    );
    let raw = transport.post(rpc_url, &body)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid RPC response JSON: {e}"))?;
    if let Some(err) = parsed.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let value = parsed
        .get("result")
        .and_then(|r| r.get("value"))
        .ok_or_else(|| "RPC response missing result.value".to_string())?;
    if value.is_null() {
        return Ok(None);
    }
    let owner = value
        .get("owner")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "account info missing owner".to_string())?
        .to_string();
    let data_b64 = value
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.as_str())
        .ok_or_else(|| "account info missing base64 data".to_string())?;
    let data = decode_base64(data_b64)?;
    Ok(Some(AccountInfo { owner, data }))
}

pub fn get_token_largest_accounts(
    transport: &dyn RpcTransport,
    rpc_url: &str,
    mint: &str,
) -> Result<Vec<LargestAccount>, String> {
    let body =
        format!(r#"{{"jsonrpc":"2.0","id":1,"method":"getTokenLargestAccounts","params":["{mint}"]}}"#);
    let raw = transport.post(rpc_url, &body)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid RPC response JSON: {e}"))?;
    if let Some(err) = parsed.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let entries = parsed
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "RPC response missing result.value array".to_string())?;
    entries
        .iter()
        .map(|entry| {
            entry
                .get("amount")
                .and_then(|a| a.as_str())
                .ok_or_else(|| "largest-account entry missing amount".to_string())
                .and_then(|s| s.parse::<u64>().map_err(|e| format!("bad amount: {e}")))
                .map(|amount| LargestAccount { amount })
        })
        .collect()
}

/// Minimal base64 (standard alphabet, with padding) decoder. No external
/// crate dependency, same rationale as `b58`.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut values = [0xFFu8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        values[c as usize] = i as u8;
    }
    let clean: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4 + 3);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for b in clean {
        let v = values[b as usize];
        if v == 0xFF {
            return Err(format!("invalid base64 character: {}", b as char));
        }
        buffer = (buffer << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{FailingTransport, MockTransport};

    #[test]
    fn parses_account_info_from_getaccountinfo_response() {
        let raw = r#"{"jsonrpc":"2.0","result":{"context":{"slot":1},"value":{"data":["AQID",""],"executable":false,"lamports":1,"owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","rentEpoch":1,"space":3}},"id":1}"#;
        let transport = MockTransport { response: raw.to_string() };
        let info = get_account_info(&transport, "http://rpc.example", "Mint111").unwrap().unwrap();
        assert_eq!(info.owner, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        assert_eq!(info.data, vec![1, 2, 3]); // base64 "AQID" decodes to [1,2,3]
    }

    #[test]
    fn missing_account_returns_none() {
        let raw = r#"{"jsonrpc":"2.0","result":{"context":{"slot":1},"value":null},"id":1}"#;
        let transport = MockTransport { response: raw.to_string() };
        let info = get_account_info(&transport, "http://rpc.example", "Mint111").unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn rpc_error_field_surfaces_as_err() {
        let raw = r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid param"},"id":1}"#;
        let transport = MockTransport { response: raw.to_string() };
        assert!(get_account_info(&transport, "http://rpc.example", "bad").is_err());
    }

    #[test]
    fn transport_failure_surfaces_as_err() {
        let transport = FailingTransport { error: "connection refused".to_string() };
        assert!(get_account_info(&transport, "http://rpc.example", "Mint111").is_err());
    }

    #[test]
    fn malformed_json_surfaces_as_err() {
        let transport = MockTransport { response: "not json".to_string() };
        assert!(get_account_info(&transport, "http://rpc.example", "Mint111").is_err());
    }

    #[test]
    fn parses_token_largest_accounts_response() {
        let raw = r#"{"jsonrpc":"2.0","result":{"context":{"slot":1},"value":[{"address":"A","amount":"500","decimals":6,"uiAmount":0.0005,"uiAmountString":"0.0005"},{"address":"B","amount":"250","decimals":6,"uiAmount":0.00025,"uiAmountString":"0.00025"}]},"id":1}"#;
        let transport = MockTransport { response: raw.to_string() };
        let accounts = get_token_largest_accounts(&transport, "http://rpc.example", "Mint111").unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].amount, 500);
        assert_eq!(accounts[1].amount, 250);
    }
}
