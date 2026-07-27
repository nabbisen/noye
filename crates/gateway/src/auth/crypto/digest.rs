//! SHA-256 digest via `globalThis.crypto.subtle.digest`.

use js_sys::{Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::jwt_verify::get_subtle;

/// Compute a SHA-256 digest (used for PKCE S256).
pub async fn sha256(input: &[u8]) -> Result<Vec<u8>, String> {
    let subtle = get_subtle()?;

    let data = Uint8Array::new_with_length(input.len() as u32);
    data.copy_from(input);

    let digest_fn = Reflect::get(&subtle, &JsValue::from_str("digest"))
        .map_err(|_| "subtle.digest missing".to_string())?;
    let digest_fn: Function = digest_fn
        .dyn_into()
        .map_err(|_| "subtle.digest is not a function".to_string())?;

    let promise = digest_fn
        .call2(&subtle, &JsValue::from_str("SHA-256"), &data)
        .map_err(|e| format!("subtle.digest call failed: {:?}", e))?;

    let result = JsFuture::from(Promise::from(promise))
        .await
        .map_err(|e| format!("subtle.digest await failed: {:?}", e))?;

    let array: Uint8Array = result
        .dyn_into()
        .map_err(|_| "digest did not return ArrayBuffer/Uint8Array".to_string())?;
    // The result is an ArrayBuffer, so wrap it in a Uint8Array.
    let wrapped = Uint8Array::new(&array);
    Ok(wrapped.to_vec())
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    //! WASM-runtime tests for SHA-256.
    //!
    //! Verified against well-known test vectors. Run with:
    //!     cargo test -p noye-gateway --target wasm32-unknown-unknown
    //!
    //! These cannot run on the host because they depend on
    //! `globalThis.crypto.subtle.digest`, which only exists inside a Workers /
    //! Node 20+ / browser runtime.

    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[wasm_bindgen_test]
    async fn empty_input_matches_known_digest() {
        // SHA-256 of "" — fixed value from FIPS 180-4 Appendix A
        let out = sha256(b"").await.expect("digest");
        assert_eq!(
            hex(&out),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[wasm_bindgen_test]
    async fn abc_matches_fips_180_4_vector() {
        // SHA-256 of "abc" — FIPS 180-4 Appendix B example 1
        let out = sha256(b"abc").await.expect("digest");
        assert_eq!(
            hex(&out),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[wasm_bindgen_test]
    async fn longer_input_matches_fips_180_4_vector() {
        // SHA-256 of "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        // — FIPS 180-4 Appendix B example 2
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let out = sha256(input).await.expect("digest");
        assert_eq!(
            hex(&out),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[wasm_bindgen_test]
    async fn output_length_is_always_32_bytes() {
        for n in [0usize, 1, 31, 32, 33, 64, 128, 1024] {
            let input = vec![0xa5_u8; n];
            let out = sha256(&input).await.expect("digest");
            assert_eq!(out.len(), 32, "len mismatch at n={}", n);
        }
    }
}
