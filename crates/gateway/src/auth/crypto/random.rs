//! Cryptographically random bytes via `globalThis.crypto.getRandomValues`.

use js_sys::{Function, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

/// Generate cryptographically random bytes (RFC 4086 quality).
///
/// Used for the PKCE verifier, state, nonce, and session IDs.
pub fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    let crypto = Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| "globalThis.crypto unavailable".to_string())?;

    let array = Uint8Array::new_with_length(len as u32);
    let get_random = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .map_err(|_| "getRandomValues missing".to_string())?;
    let get_random: Function = get_random
        .dyn_into()
        .map_err(|_| "getRandomValues is not a function".to_string())?;

    get_random
        .call1(&crypto, &array)
        .map_err(|e| format!("getRandomValues failed: {:?}", e))?;

    Ok(array.to_vec())
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    //! WASM-runtime tests for the cryptographic RNG.
    //!
    //! Statistical quality of `getRandomValues` is the runtime's
    //! responsibility; what we verify here is that we wire it up correctly
    //! and that the output meets the bare-minimum non-trivial-output
    //! criteria.

    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    #[wasm_bindgen_test]
    fn returns_requested_length() {
        for n in [0usize, 1, 16, 32, 64, 256] {
            let bytes = random_bytes(n).expect("random_bytes");
            assert_eq!(bytes.len(), n, "length mismatch at n={}", n);
        }
    }

    #[wasm_bindgen_test]
    fn two_independent_calls_produce_distinct_output() {
        // The probability of two independent 32-byte draws colliding is
        // ~2^-256. A failure here means we are not actually calling the RNG.
        let a = random_bytes(32).expect("a");
        let b = random_bytes(32).expect("b");
        assert_ne!(a, b);
    }

    #[wasm_bindgen_test]
    fn output_is_not_all_zero() {
        // Same idea as above: an all-zero 32-byte output is possible (~2^-256)
        // but observing it would mean the RNG isn't working.
        let bytes = random_bytes(32).expect("rb");
        assert!(bytes.iter().any(|&b| b != 0), "RNG returned 32 zero bytes");
    }
}
