//! Base64URL encode/decode (RFC 7515 Appendix C, no padding).

/// Base64URL encode (no padding; per RFC 7515).
pub fn base64url_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Base64URL decode (padding is restored automatically before decoding).
pub fn base64url_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    // Restore padding so the standard URL_SAFE alphabet can decode.
    let mut s = input.to_string();
    match s.len() % 4 {
        2 => s.push_str("=="),
        3 => s.push('='),
        0 => {}
        _ => return Err("Invalid base64url length".to_string()),
    }
    base64::engine::general_purpose::URL_SAFE
        .decode(&s)
        .map_err(|e| format!("base64url decode error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_arbitrary_bytes() {
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"hello world",
            &[0xff, 0xfe, 0xfd, 0xfc, 0x00, 0x01],
        ];
        for input in cases {
            let encoded = base64url_encode(input);
            let decoded = base64url_decode(&encoded).expect("round-trip should succeed");
            assert_eq!(
                decoded.as_slice(),
                *input,
                "round-trip failed for {:?}",
                input
            );
        }
    }

    #[test]
    fn encode_uses_url_safe_alphabet_without_padding() {
        // 0xff 0xff 0xff produces "////" in standard base64; URL-safe replaces with "____".
        let encoded = base64url_encode(&[0xff, 0xff, 0xff]);
        assert_eq!(encoded, "____");
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
    }

    #[test]
    fn decode_accepts_input_without_padding() {
        // "Zm9v" is "foo" with no padding (length divisible by 4 after restoration).
        assert_eq!(base64url_decode("Zm9v").unwrap(), b"foo");
        // "Zg" decodes to "f" — needs ".." padding restored.
        assert_eq!(base64url_decode("Zg").unwrap(), b"f");
        // "Zm8" decodes to "fo" — needs "." padding restored.
        assert_eq!(base64url_decode("Zm8").unwrap(), b"fo");
    }

    #[test]
    fn decode_rejects_invalid_length() {
        // length % 4 == 1 is impossible for a valid base64 string.
        let result = base64url_decode("Z");
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_invalid_characters() {
        // "!" is not in the base64 alphabet.
        let result = base64url_decode("Zm9!");
        assert!(result.is_err());
    }

    #[test]
    fn decode_handles_url_safe_special_characters() {
        // Bytes 0xFB 0xEF 0xFF round-trip through URL-safe encoding (- and _).
        let encoded = base64url_encode(&[0xfb, 0xef, 0xff]);
        // Standard base64 of these bytes is "++//"; URL-safe is "--__".
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        let decoded = base64url_decode(&encoded).unwrap();
        assert_eq!(decoded, vec![0xfb, 0xef, 0xff]);
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    //! Same round-trip property as the host-target tests, re-checked under
    //! WASM. Cheap insurance against a future change to a base64 backend with
    //! divergent WASM behavior.

    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    #[wasm_bindgen_test]
    fn round_trip_under_wasm() {
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"abc",
            b"hello world",
            &[0xff, 0xfe, 0xfd, 0x00, 0x01],
        ];
        for input in cases {
            let encoded = base64url_encode(input);
            let decoded = base64url_decode(&encoded).expect("round-trip");
            assert_eq!(decoded.as_slice(), *input);
        }
    }
}
