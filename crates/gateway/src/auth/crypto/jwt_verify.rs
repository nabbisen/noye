//! JWT signature verification using a JWK public key.

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// Verify a signature using a JWK-formatted public key.
///
/// Primarily intended for OIDC ID Token RS256 (RSASSA-PKCS1-v1_5 + SHA-256),
/// but the algorithm is taken from the JWK or the JWT header `alg` field.
///
/// # Arguments
///
/// * `jwk_json` - A single JWK selected from the JWKS
/// * `alg` - The `alg` claim from the JWT header (RS256/RS384/RS512/ES256, etc.)
/// * `signing_input` - The signing input (the ASCII string `header.payload`)
/// * `signature` - The Base64URL-decoded signature bytes
pub async fn verify_jwt_signature(
    jwk_json: &serde_json::Value,
    alg: &str,
    signing_input: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    let subtle = get_subtle()?;

    // Map the algorithm to Web Crypto parameters.
    let (algo_name, hash_name) = match alg {
        "RS256" => ("RSASSA-PKCS1-v1_5", "SHA-256"),
        "RS384" => ("RSASSA-PKCS1-v1_5", "SHA-384"),
        "RS512" => ("RSASSA-PKCS1-v1_5", "SHA-512"),
        "PS256" => ("RSA-PSS", "SHA-256"),
        "ES256" => ("ECDSA", "SHA-256"),
        "ES384" => ("ECDSA", "SHA-384"),
        other => return Err(format!("Unsupported JWT alg: {}", other)),
    };

    // Construct the algorithm parameter for importKey.
    let key_algo = Object::new();
    Reflect::set(&key_algo, &JsValue::from_str("name"), &JsValue::from_str(algo_name))
        .map_err(|_| "Failed to set algo name".to_string())?;
    let hash_obj = Object::new();
    Reflect::set(&hash_obj, &JsValue::from_str("name"), &JsValue::from_str(hash_name))
        .map_err(|_| "Failed to set hash name".to_string())?;
    Reflect::set(&key_algo, &JsValue::from_str("hash"), &hash_obj)
        .map_err(|_| "Failed to set hash".to_string())?;

    // ECDSA also requires namedCurve.
    if algo_name == "ECDSA" {
        let curve = match alg {
            "ES256" => "P-256",
            "ES384" => "P-384",
            _ => "P-256",
        };
        Reflect::set(
            &key_algo,
            &JsValue::from_str("namedCurve"),
            &JsValue::from_str(curve),
        )
        .map_err(|_| "Failed to set namedCurve".to_string())?;
    }

    // Convert the JWK to a JsValue.
    let jwk_js = js_sys::JSON::parse(&jwk_json.to_string())
        .map_err(|e| format!("JWK parse failed: {:?}", e))?;

    // key_usages: ["verify"]
    let usages = Array::new();
    usages.push(&JsValue::from_str("verify"));

    // importKey("jwk", jwk, algorithm, extractable, keyUsages)
    let import_key_fn = Reflect::get(&subtle, &JsValue::from_str("importKey"))
        .map_err(|_| "subtle.importKey missing".to_string())?;
    let import_key_fn: Function = import_key_fn
        .dyn_into()
        .map_err(|_| "importKey is not a function".to_string())?;

    // importKey takes a variadic argument list, so we use apply.
    let args = Array::new();
    args.push(&JsValue::from_str("jwk"));
    args.push(&jwk_js);
    args.push(&key_algo);
    args.push(&JsValue::FALSE);
    args.push(&usages);

    let promise = import_key_fn
        .apply(&subtle, &args)
        .map_err(|e| format!("importKey call failed: {:?}", e))?;

    let key = JsFuture::from(Promise::from(promise))
        .await
        .map_err(|e| format!("importKey failed: {:?}", e))?;

    // Algorithm argument for verify.
    let verify_algo = Object::new();
    Reflect::set(&verify_algo, &JsValue::from_str("name"), &JsValue::from_str(algo_name))
        .map_err(|_| "Failed to set verify algo name".to_string())?;
    if algo_name == "RSA-PSS" {
        Reflect::set(&verify_algo, &JsValue::from_str("saltLength"), &JsValue::from(32))
            .map_err(|_| "Failed to set saltLength".to_string())?;
    }
    if algo_name == "ECDSA" {
        Reflect::set(&verify_algo, &JsValue::from_str("hash"), &hash_obj)
            .map_err(|_| "Failed to set ECDSA hash".to_string())?;
    }

    // Wrap signature and signing_input as Uint8Arrays.
    let sig_array = Uint8Array::new_with_length(signature.len() as u32);
    sig_array.copy_from(signature);
    let data_array = Uint8Array::new_with_length(signing_input.len() as u32);
    data_array.copy_from(signing_input);

    // verify(algorithm, key, signature, data)
    let verify_fn = Reflect::get(&subtle, &JsValue::from_str("verify"))
        .map_err(|_| "subtle.verify missing".to_string())?;
    let verify_fn: Function = verify_fn
        .dyn_into()
        .map_err(|_| "verify is not a function".to_string())?;

    let args = Array::new();
    args.push(&verify_algo);
    args.push(&key);
    args.push(&sig_array);
    args.push(&data_array);

    let promise = verify_fn
        .apply(&subtle, &args)
        .map_err(|e| format!("verify call failed: {:?}", e))?;

    let result = JsFuture::from(Promise::from(promise))
        .await
        .map_err(|e| format!("verify await failed: {:?}", e))?;

    Ok(result.is_truthy())
}

/// Resolve `globalThis.crypto.subtle`. Used by the digest and verify paths.
pub(super) fn get_subtle() -> Result<JsValue, String> {
    let crypto = Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| "globalThis.crypto unavailable".to_string())?;
    Reflect::get(&crypto, &JsValue::from_str("subtle"))
        .map_err(|_| "crypto.subtle unavailable".to_string())
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    //! WASM-runtime tests for JWT signature verification.
    //!
    //! Test vectors come from RFC 7515 (JSON Web Signature), Appendix A.2,
    //! which provides a complete RS256 example: a JWK, a signing input, and
    //! the resulting signature in a single self-contained reference.

    use super::*;
    use crate::auth::crypto::base64url::base64url_decode;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    /// JWK from RFC 7515 §A.2.1 (public-key fields only — Web Crypto's
    /// `importKey` accepts the public form for `verify` usage). The original
    /// example uses both public and private fields; we reduce to just the
    /// public ones here.
    fn rfc7515_a2_jwk() -> serde_json::Value {
        serde_json::json!({
            "kty": "RSA",
            "n": "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ",
            "e": "AQAB",
            "alg": "RS256",
            "kid": "rfc7515-a2"
        })
    }

    /// Signing input from RFC 7515 §A.2 — `protected.payload` joined by `.`,
    /// in ASCII bytes. The example header is `{"alg":"RS256"}` and the
    /// example payload is the JSON `{"iss":"joe","exp":1300819380,"http://example.com/is_root":true}`,
    /// each base64url-encoded.
    fn rfc7515_a2_signing_input() -> &'static [u8] {
        b"eyJhbGciOiJSUzI1NiJ9.\
          eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ"
    }

    /// Detached signature from RFC 7515 §A.2.1, base64url-encoded (no padding).
    fn rfc7515_a2_signature_b64u() -> &'static str {
        "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7\
         AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4\
         BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K\
         0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqv\
         hJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrB\
         p0igcN_IoypGlUPQGe77Rw"
    }

    fn signing_input_compacted() -> Vec<u8> {
        // The string above contains internal whitespace from line breaks in
        // the source; strip it before passing to the verifier.
        rfc7515_a2_signing_input()
            .iter()
            .copied()
            .filter(|&b| !b.is_ascii_whitespace())
            .collect()
    }

    fn signature_bytes() -> Vec<u8> {
        let s: String = rfc7515_a2_signature_b64u()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        base64url_decode(&s).expect("decode signature")
    }

    #[wasm_bindgen_test]
    async fn rs256_verifies_valid_signature() {
        let ok = verify_jwt_signature(
            &rfc7515_a2_jwk(),
            "RS256",
            &signing_input_compacted(),
            &signature_bytes(),
        )
        .await
        .expect("verify call");
        assert!(ok, "RFC 7515 A.2 signature did not verify");
    }

    #[wasm_bindgen_test]
    async fn rs256_rejects_tampered_payload() {
        // Flip one byte of the signing input — verify must return false.
        let mut tampered = signing_input_compacted();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let ok = verify_jwt_signature(&rfc7515_a2_jwk(), "RS256", &tampered, &signature_bytes())
            .await
            .expect("verify call");
        assert!(!ok, "tampered payload was accepted");
    }

    #[wasm_bindgen_test]
    async fn rs256_rejects_tampered_signature() {
        let mut tampered = signature_bytes();
        tampered[0] ^= 0xff;
        let ok = verify_jwt_signature(
            &rfc7515_a2_jwk(),
            "RS256",
            &signing_input_compacted(),
            &tampered,
        )
        .await
        .expect("verify call");
        assert!(!ok, "tampered signature was accepted");
    }

    #[wasm_bindgen_test]
    async fn unsupported_alg_returns_err() {
        // None: HS256 (HMAC) is not allowed because OIDC ID Tokens must use
        // an asymmetric algorithm. The verifier should reject the alg outright
        // rather than silently fall through to a different algorithm.
        let res = verify_jwt_signature(
            &rfc7515_a2_jwk(),
            "HS256",
            &signing_input_compacted(),
            &signature_bytes(),
        )
        .await;
        assert!(res.is_err(), "HS256 should be rejected");
        let msg = res.err().unwrap();
        assert!(msg.contains("Unsupported"), "error message should mention support: {}", msg);
    }

    #[wasm_bindgen_test]
    async fn wrong_key_rejects_signature() {
        // A different (validly-formed) RSA public key from the same family
        // should fail to verify against an RFC-7515-A.2 signature.
        let other_jwk = serde_json::json!({
            "kty": "RSA",
            // Different modulus. This is a deliberately malformed-but-plausible
            // public key generated by changing a couple of base64url chars in
            // the original `n`. Web Crypto's importKey accepts the format,
            // but verify will (correctly) report mismatch.
            "n": "BfgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ",
            "e": "AQAB",
            "alg": "RS256",
            "kid": "wrong-key"
        });
        let ok = verify_jwt_signature(
            &other_jwk,
            "RS256",
            &signing_input_compacted(),
            &signature_bytes(),
        )
        .await
        .expect("verify call");
        assert!(!ok, "verification should fail with a different public key");
    }
}
