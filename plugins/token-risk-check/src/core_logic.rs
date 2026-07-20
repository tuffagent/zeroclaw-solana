//! Composes `solana-core`'s RPC calls, mint parsing, and risk scoring into
//! the full risk-check flow. No wasm dependency, no `waki` — takes an
//! `RpcTransport` by reference so this whole module is host-testable with
//! a mock, per the bounty's hard requirement.

use solana_core::mint::{self, MintExtensions};
use solana_core::risk::{self, ConcentrationInput, Verdict};
use solana_core::rpc::{self, LargestAccount};
use solana_core::transport::RpcTransport;
use solana_core::b58;

use crate::config::RiskCheckConfig;

#[derive(Debug, Clone)]
pub struct RiskCheckOutput {
    pub score: f64,
    pub verdict: Verdict,
    pub mint_authority_present: bool,
    pub freeze_authority_present: bool,
    pub extensions: MintExtensions,
    pub concentration: ConcentrationInput,
    pub decimals: u8,
    pub supply: u64,
}

pub fn run(
    transport: &dyn RpcTransport,
    config: &RiskCheckConfig,
    mint_pubkey: &str,
) -> Result<RiskCheckOutput, String> {
    let decoded = b58::decode(mint_pubkey)
        .map_err(|e| format!("'{mint_pubkey}' is not a valid base58 pubkey: {e}"))?;
    if decoded.len() != 32 {
        return Err(format!(
            "'{mint_pubkey}' is not a valid 32-byte base58 pubkey ({} bytes decoded)",
            decoded.len()
        ));
    }

    let account = rpc::get_account_info(transport, &config.rpc_url, mint_pubkey)?
        .ok_or_else(|| format!("no account found for mint {mint_pubkey}"))?;

    let parsed = mint::parse_mint(&account.data, &account.owner)?;

    let largest = rpc::get_token_largest_accounts(transport, &config.rpc_url, mint_pubkey)?;
    let concentration = concentration_from_largest(&largest, parsed.supply);

    let (score, verdict) = risk::score(
        parsed.authorities.mint_authority_present,
        parsed.authorities.freeze_authority_present,
        &parsed.extensions,
        &concentration,
        config.amber_threshold,
        config.red_threshold,
    );

    Ok(RiskCheckOutput {
        score,
        verdict,
        mint_authority_present: parsed.authorities.mint_authority_present,
        freeze_authority_present: parsed.authorities.freeze_authority_present,
        extensions: parsed.extensions,
        concentration,
        decimals: parsed.decimals,
        supply: parsed.supply,
    })
}

/// `getTokenLargestAccounts` returns entries already sorted descending by
/// amount, so the first N entries are exactly the top N holders.
fn concentration_from_largest(largest: &[LargestAccount], total_supply: u64) -> ConcentrationInput {
    if total_supply == 0 {
        return ConcentrationInput {
            top1_pct: 0.0,
            top5_pct: 0.0,
            top10_pct: 0.0,
            top20_pct: 0.0,
        };
    }
    let pct_of = |n: usize| -> f64 {
        let sum: u128 = largest.iter().take(n).map(|a| a.amount as u128).sum();
        (sum as f64 / total_supply as f64) * 100.0
    };
    ConcentrationInput {
        top1_pct: pct_of(1),
        top5_pct: pct_of(5),
        top10_pct: pct_of(10),
        top20_pct: pct_of(20),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // A transport that returns a different canned response depending on
    // which RPC method the request body names, so one test can drive both
    // getAccountInfo and getTokenLargestAccounts calls in sequence.
    struct SequencedTransport {
        account_info: String,
        largest_accounts: String,
    }
    impl solana_core::transport::RpcTransport for SequencedTransport {
        fn post(&self, _url: &str, body: &str) -> Result<String, String> {
            if body.contains("getAccountInfo") {
                Ok(self.account_info.clone())
            } else {
                Ok(self.largest_accounts.clone())
            }
        }
    }

    fn clean_mint_account_info() -> String {
        // 82-byte legacy layout: no authorities, supply 1_000_000, decimals 6.
        // base64 of [0,0,0,0]+[0;32]+1_000_000u64LE+[6]+[1]+[0,0,0,0]+[0;32]
        r#"{"jsonrpc":"2.0","result":{"value":{"owner":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA","data":["AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQEIPAAAAAAAGAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",""]}},"id":1}"#.to_string()
    }

    fn largest_accounts(amounts: &[u64]) -> String {
        let entries: Vec<String> = amounts
            .iter()
            .map(|a| format!(r#"{{"address":"x","amount":"{a}","decimals":6,"uiAmount":0,"uiAmountString":"0"}}"#))
            .collect();
        format!(r#"{{"jsonrpc":"2.0","result":{{"value":[{}]}},"id":1}}"#, entries.join(","))
    }

    #[test]
    fn clean_mint_with_low_concentration_scores_green() {
        let transport = SequencedTransport {
            account_info: clean_mint_account_info(),
            largest_accounts: largest_accounts(&[30_000, 30_000, 30_000, 30_000, 30_000]), // 5 * 30_000 = 150_000 of 1_000_000
        };
        let config = RiskCheckConfig::from_section(&HashMap::new());
        let result = run(&transport, &config, "11111111111111111111111111111111").unwrap();
        assert!(!result.mint_authority_present);
        assert!(!result.freeze_authority_present);
        assert_eq!(result.decimals, 6);
        assert_eq!(result.supply, 1_000_000);
        assert_eq!(result.verdict, solana_core::risk::Verdict::Green);
    }

    #[test]
    fn invalid_mint_address_is_rejected_before_any_network_call() {
        let transport = SequencedTransport {
            account_info: "should never be read".to_string(),
            largest_accounts: "should never be read".to_string(),
        };
        let config = RiskCheckConfig::from_section(&HashMap::new());
        let err = run(&transport, &config, "not-valid-base58-!!!").unwrap_err();
        assert!(err.contains("not a valid"), "unexpected error: {err}");
    }

    #[test]
    fn missing_account_is_a_clear_error() {
        let transport = SequencedTransport {
            account_info: r#"{"jsonrpc":"2.0","result":{"value":null},"id":1}"#.to_string(),
            largest_accounts: largest_accounts(&[]),
        };
        let config = RiskCheckConfig::from_section(&HashMap::new());
        let err = run(&transport, &config, "11111111111111111111111111111111").unwrap_err();
        assert!(err.contains("no account found"), "unexpected error: {err}");
    }
}
