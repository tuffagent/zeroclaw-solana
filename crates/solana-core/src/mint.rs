//! SPL Token / Token-2022 mint account parsing, hand-rolled against the
//! real on-chain byte layout rather than depending on `spl-token`/
//! `spl-token-2022` (unproven dependency chains for wasm32-wasip2). Offsets
//! and extension-type ordinals below were confirmed against the
//! `solana-program/token-2022` source while writing this plan.

pub const LEGACY_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

const MINT_BASE_LEN: usize = 82;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MintAuthorities {
    pub mint_authority_present: bool,
    pub freeze_authority_present: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MintExtensions {
    pub permanent_delegate: bool,
    pub transfer_hook: bool,
    pub transfer_fee_config: bool,
    pub default_account_state_frozen: bool,
    pub non_transferable: bool,
    pub confidential_transfer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedMint {
    pub supply: u64,
    pub decimals: u8,
    pub authorities: MintAuthorities,
    pub extensions: MintExtensions,
}

/// Parse a decoded mint account's raw byte data (already base64-decoded by
/// the caller). `owner_program` is the account's `owner` field from
/// `getAccountInfo`, used to decide whether to look for Token-2022
/// extensions past the base 82 bytes.
pub fn parse_mint(data: &[u8], owner_program: &str) -> Result<ParsedMint, String> {
    if data.len() < MINT_BASE_LEN {
        return Err(format!(
            "mint account data too short: {} bytes, need at least {MINT_BASE_LEN}",
            data.len()
        ));
    }

    let authorities = MintAuthorities {
        mint_authority_present: data[0..4] == [1, 0, 0, 0],
        freeze_authority_present: data[46..50] == [1, 0, 0, 0],
    };
    let supply = u64::from_le_bytes(data[36..44].try_into().unwrap());
    let decimals = data[44];

    let extensions = if owner_program == TOKEN_2022_PROGRAM && data.len() > MINT_BASE_LEN {
        parse_extensions(data)?
    } else {
        MintExtensions::default()
    };

    Ok(ParsedMint {
        supply,
        decimals,
        authorities,
        extensions,
    })
}

const ACCOUNT_TYPE_OFFSET: usize = 165;
const TLV_START_OFFSET: usize = ACCOUNT_TYPE_OFFSET + 1;

fn parse_extensions(data: &[u8]) -> Result<MintExtensions, String> {
    if data.len() < TLV_START_OFFSET {
        return Err(format!(
            "mint account data ({} bytes) too short for a Token-2022 account-type byte at offset {ACCOUNT_TYPE_OFFSET}",
            data.len()
        ));
    }
    if data[MINT_BASE_LEN..ACCOUNT_TYPE_OFFSET].iter().any(|&b| b != 0) {
        return Err("non-zero padding before the Token-2022 account-type byte".to_string());
    }
    let account_type = data[ACCOUNT_TYPE_OFFSET];
    if account_type != 1 {
        // 0 = Uninitialized, 1 = Mint, 2 = Account.
        return Err(format!("unexpected Token-2022 account-type byte: {account_type}"));
    }

    let mut extensions = MintExtensions::default();
    let mut offset = TLV_START_OFFSET;
    while offset + 4 <= data.len() {
        let extension_type = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap());
        let length = u16::from_le_bytes(data[offset + 2..offset + 4].try_into().unwrap()) as usize;
        let value_start = offset + 4;
        let value_end = value_start + length;
        if value_end > data.len() {
            return Err(format!(
                "Token-2022 extension {extension_type} declares length {length} past end of account data"
            ));
        }
        match extension_type {
            1 => extensions.transfer_fee_config = true,
            4 => extensions.confidential_transfer = true,
            6 => {
                if let Some(&state_byte) = data.get(value_start) {
                    extensions.default_account_state_frozen = state_byte == 2;
                }
            }
            9 => extensions.non_transferable = true,
            12 => extensions.permanent_delegate = true,
            14 => extensions.transfer_hook = true,
            _ => {} // other extension types don't feed the risk model
        }
        offset = value_end;
    }
    Ok(extensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_mint_fixture() -> Vec<u8> {
        // base64: AQAAAKqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqABCl1OgAAAAGAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==
        let mut b = Vec::new();
        b.extend([1, 0, 0, 0]);
        b.extend([0xAA; 32]); // mint_authority Some
        b.extend(1_000_000_000_000u64.to_le_bytes()); // supply
        b.push(6); // decimals
        b.push(1); // is_initialized
        b.extend([0, 0, 0, 0]);
        b.extend([0u8; 32]); // freeze_authority None
        assert_eq!(b.len(), 82);
        b
    }

    fn tlv(t: u16, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(t.to_le_bytes());
        out.extend((value.len() as u16).to_le_bytes());
        out.extend(value);
        out
    }

    fn token2022_ext_mint_fixture() -> Vec<u8> {
        // base64: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGXNHQAAAAAJAQEAAAC7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7u7uwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQwAIADMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzA4AQADd3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3d3dAQAQAO7u7u7u7u7u7u7u7u7u7u4GAAEAAgkAAAAEAAgA//////////8=
        let mut b = Vec::new();
        b.extend([0, 0, 0, 0]);
        b.extend([0u8; 32]); // mint_authority None
        b.extend(500_000_000u64.to_le_bytes()); // supply
        b.push(9); // decimals
        b.push(1); // is_initialized
        b.extend([1, 0, 0, 0]);
        b.extend([0xBB; 32]); // freeze_authority Some
        assert_eq!(b.len(), 82);
        b.extend([0u8; 83]); // padding, offsets 82..165
        b.push(1); // account_type = Mint
        b.extend(tlv(12, &[0xCC; 32])); // PermanentDelegate
        b.extend(tlv(14, &[0xDD; 64])); // TransferHook
        b.extend(tlv(1, &[0xEE; 16])); // TransferFeeConfig
        b.extend(tlv(6, &[2])); // DefaultAccountState = Frozen
        b.extend(tlv(9, &[])); // NonTransferable, zero length
        b.extend(tlv(4, &[0xFF; 8])); // ConfidentialTransferMint
        assert_eq!(b.len(), 311);
        b
    }

    #[test]
    fn parses_legacy_mint_authorities_supply_decimals() {
        let data = legacy_mint_fixture();
        let parsed = parse_mint(&data, LEGACY_TOKEN_PROGRAM).expect("valid mint");
        assert!(parsed.authorities.mint_authority_present);
        assert!(!parsed.authorities.freeze_authority_present);
        assert_eq!(parsed.supply, 1_000_000_000_000);
        assert_eq!(parsed.decimals, 6);
        assert_eq!(parsed.extensions, MintExtensions::default());
    }

    #[test]
    fn rejects_data_shorter_than_82_bytes() {
        let short = vec![0u8; 81];
        assert!(parse_mint(&short, LEGACY_TOKEN_PROGRAM).is_err());
    }

    #[test]
    fn parses_token_2022_extensions() {
        let data = token2022_ext_mint_fixture();
        let parsed = parse_mint(&data, TOKEN_2022_PROGRAM).expect("valid mint");
        assert!(!parsed.authorities.mint_authority_present);
        assert!(parsed.authorities.freeze_authority_present);
        assert_eq!(parsed.supply, 500_000_000);
        assert_eq!(parsed.decimals, 9);
        assert!(parsed.extensions.permanent_delegate);
        assert!(parsed.extensions.transfer_hook);
        assert!(parsed.extensions.transfer_fee_config);
        assert!(parsed.extensions.default_account_state_frozen);
        assert!(parsed.extensions.non_transferable);
        assert!(parsed.extensions.confidential_transfer);
    }

    #[test]
    fn token_2022_mint_with_no_extensions_is_exactly_82_bytes() {
        let data = legacy_mint_fixture(); // same 82-byte shape, different owner
        let parsed = parse_mint(&data, TOKEN_2022_PROGRAM).expect("valid mint");
        assert_eq!(parsed.extensions, MintExtensions::default());
    }

    #[test]
    fn rejects_non_zero_padding_before_account_type() {
        let mut data = token2022_ext_mint_fixture();
        data[100] = 0x01; // corrupt a byte inside the 82..165 padding region
        let truncated: Vec<u8> = data[..166].to_vec();
        assert!(parse_mint(&truncated, TOKEN_2022_PROGRAM).is_err());
    }

    #[test]
    fn rejects_tlv_length_past_end_of_data() {
        let mut data = vec![0u8; 82]; // minimal all-zero base
        data.extend([0u8; 83]); // padding
        data.push(1); // account_type = Mint
        data.extend(1u16.to_le_bytes()); // extension_type = TransferFeeConfig
        data.extend(1000u16.to_le_bytes()); // claims length 1000, no value bytes follow
        assert!(parse_mint(&data, TOKEN_2022_PROGRAM).is_err());
    }
}
