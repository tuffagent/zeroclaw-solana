//! Hand-rolled base58 (Bitcoin alphabet), used for Solana pubkeys. No
//! external crate dependency: this is the exact algorithm bs58-family
//! crates use (array-based long division for encode, multiply-accumulate
//! for decode), verified byte-for-byte against an independent reference
//! implementation while writing this plan.

const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn digit_value(ch: u8) -> Option<u8> {
    ALPHABET.iter().position(|&a| a == ch).map(|i| i as u8)
}

/// Encode raw bytes as base58. Leading zero bytes become leading '1'
/// characters and contribute no other output (a 32-byte all-zero pubkey,
/// the System Program, encodes as exactly 32 '1' characters).
pub fn encode(input: &[u8]) -> String {
    let leading_zeros = input.iter().take_while(|&&b| b == 0).count();
    let mut num: Vec<u8> = input.to_vec();
    let mut digits: Vec<u8> = Vec::new();
    while !num.iter().all(|&b| b == 0) {
        let mut remainder: u32 = 0;
        for byte in num.iter_mut() {
            let value = remainder * 256 + (*byte as u32);
            *byte = (value / 58) as u8;
            remainder = value % 58;
        }
        digits.push(remainder as u8);
    }
    let mut out: Vec<u8> = vec![ALPHABET[0]; leading_zeros];
    out.extend(digits.iter().rev().map(|&d| ALPHABET[d as usize]));
    String::from_utf8(out).expect("alphabet is ASCII")
}

/// Decode a base58 string back to raw bytes. Errors on any character
/// outside the 58-character alphabet (notably `0`, `O`, `I`, `l`, which are
/// deliberately excluded from it).
pub fn decode(input: &str) -> Result<Vec<u8>, String> {
    let leading_ones = input.bytes().take_while(|&b| b == ALPHABET[0]).count();
    let mut acc: Vec<u8> = Vec::new();
    for byte in input.bytes() {
        let mut carry = digit_value(byte)
            .ok_or_else(|| format!("invalid base58 character: {}", byte as char))?
            as u32;
        for slot in acc.iter_mut() {
            let value = (*slot as u32) * 58 + carry;
            *slot = (value & 0xFF) as u8;
            carry = value >> 8;
        }
        while carry > 0 {
            acc.push((carry & 0xFF) as u8);
            carry >>= 8;
        }
    }
    acc.reverse();
    let mut out = vec![0u8; leading_ones];
    out.extend(acc);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_all_zero_32_bytes_as_32_ones() {
        let input = [0u8; 32];
        let encoded = encode(&input);
        assert_eq!(encoded, "1".repeat(32));
        assert_eq!(encoded.len(), 32);
    }

    #[test]
    fn encodes_known_vectors() {
        let mut v = [0u8; 32];
        v[0] = 1;
        assert_eq!(encode(&v), "4uQeVj5tqViQh7yWWGStvkEG1Zmhx6uasJtWCJziofM");

        let v: Vec<u8> = (1u8..=32).collect();
        assert_eq!(encode(&v), "4wBqpZM9xaSheZzJSMawUKKwhdpChKbZ5eu5ky4Vigw");

        let mut v = vec![0u8, 0u8];
        v.extend(1u8..=30);
        assert_eq!(encode(&v), "11CiMQsCUhqABwwLyCFeX2iPnBZX3s28dUUCBrirhs");
    }

    #[test]
    fn round_trips_edge_and_random_vectors() {
        let vectors: [&[u8]; 4] = [
            &[0u8; 32],
            &[255u8; 32],
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            &[0, 0, 0, 9, 8, 7],
        ];
        for v in vectors {
            let encoded = encode(v);
            let decoded = decode(&encoded).expect("valid base58");
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn decode_rejects_invalid_characters() {
        // 0, O, I, l are all deliberately excluded from the alphabet.
        assert!(decode("0OIl").is_err());
    }

    #[test]
    fn decode_empty_string_is_empty_bytes() {
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }
}
