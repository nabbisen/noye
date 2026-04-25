//! Web Crypto API (`globalThis.crypto`, `globalThis.crypto.subtle`) のラッパー。
//!
//! Workers 環境では `ring` などネイティブな暗号 crate が利用できないため、
//! ランタイムが提供する SubtleCrypto を JS バインディング経由で呼び出す。
//! 透明性のため web-sys の高レベルバインディングは使わず、
//! TCP/SMTP モジュールと同じく `js_sys::Reflect` による直接呼び出しで統一する。

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// 暗号学的乱数を生成する (RFC 4086 品質)。
///
/// PKCE verifier や state, nonce, session_id に使用。
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

/// SHA-256 ハッシュを計算する (PKCE S256 で使用)。
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
    // result は ArrayBuffer なので Uint8Array でラップし直す
    let wrapped = Uint8Array::new(&array);
    Ok(wrapped.to_vec())
}

/// JWK 形式の公開鍵で署名を検証する。
///
/// OIDC ID Token の RS256 (RSASSA-PKCS1-v1_5 + SHA-256) 検証を主対象とするが、
/// アルゴリズムは JWK の `alg` または JWT ヘッダの `alg` から決定する。
///
/// # Arguments
/// * `jwk_json` - JWKS から選択された単一の JWK (serde_json::Value)
/// * `alg` - JWT ヘッダの alg クレーム (RS256/RS384/RS512/ES256 等)
/// * `signing_input` - 署名対象 (`header.payload` の ASCII 文字列)
/// * `signature` - Base64URL デコード済みの署名バイト列
pub async fn verify_jwt_signature(
    jwk_json: &serde_json::Value,
    alg: &str,
    signing_input: &[u8],
    signature: &[u8],
) -> Result<bool, String> {
    let subtle = get_subtle()?;

    // アルゴリズム → Web Crypto パラメータへのマッピング
    let (algo_name, hash_name) = match alg {
        "RS256" => ("RSASSA-PKCS1-v1_5", "SHA-256"),
        "RS384" => ("RSASSA-PKCS1-v1_5", "SHA-384"),
        "RS512" => ("RSASSA-PKCS1-v1_5", "SHA-512"),
        "PS256" => ("RSA-PSS", "SHA-256"),
        "ES256" => ("ECDSA", "SHA-256"),
        "ES384" => ("ECDSA", "SHA-384"),
        other => return Err(format!("Unsupported JWT alg: {}", other)),
    };

    // importKey の algorithm パラメータを構築
    let key_algo = Object::new();
    Reflect::set(&key_algo, &JsValue::from_str("name"), &JsValue::from_str(algo_name))
        .map_err(|_| "Failed to set algo name".to_string())?;
    let hash_obj = Object::new();
    Reflect::set(&hash_obj, &JsValue::from_str("name"), &JsValue::from_str(hash_name))
        .map_err(|_| "Failed to set hash name".to_string())?;
    Reflect::set(&key_algo, &JsValue::from_str("hash"), &hash_obj)
        .map_err(|_| "Failed to set hash".to_string())?;

    // ECDSA の場合は namedCurve も必要
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

    // JWK を JsValue に変換
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

    // importKey は可変長引数なので apply を使う
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

    // verify の algorithm 引数
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

    // signature と signing_input を Uint8Array に
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

fn get_subtle() -> Result<JsValue, String> {
    let crypto = Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| "globalThis.crypto unavailable".to_string())?;
    Reflect::get(&crypto, &JsValue::from_str("subtle"))
        .map_err(|_| "crypto.subtle unavailable".to_string())
}

/// Base64URL エンコード (パディングなし、RFC 7515 準拠)。
pub fn base64url_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Base64URL デコード (パディング自動補完)。
pub fn base64url_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    // パディング補完
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
