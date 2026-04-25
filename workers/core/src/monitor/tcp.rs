use worker::*;

use noye_shared::Target;
use super::CheckOutcome;

/// TCP ヘルスチェック (要件2-3)
///
/// 検証項目:
/// - 指定ポートへの接続確立
/// - タイムアウト未発生
/// - バナー応答確認 (body_contains が設定されている場合)
///
/// Cloudflare Workers の connect() API (TCP Sockets) を使用。
/// 注意: Workers 環境での TCP ソケットは connect() API 経由で利用可能。
///       ただし wasm32-unknown-unknown ターゲットでのビルド互換性を事前検証のこと。
pub async fn check(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;

    let port = match target.port {
        Some(p) => p as u16,
        None => {
            return CheckOutcome::failure(
                "TCP check requires a port number".to_string(),
                0,
            );
        }
    };

    let address = format!("{}:{}", target.host, port);

    // Cloudflare Workers TCP connect() API
    // JS バインディング経由で実行
    let result = tcp_connect_js(&target.host, port, target.timeout_sec as u32).await;

    let elapsed = (js_sys::Date::now() as i64) - start;

    match result {
        Ok(banner_opt) => {
            // タイムアウト判定
            let timeout_ms = (target.timeout_sec * 1000) as i64;
            if elapsed > timeout_ms {
                return CheckOutcome::failure(
                    format!("TCP timeout: {}ms > {}ms limit", elapsed, timeout_ms),
                    elapsed,
                );
            }

            // バナー応答確認 (オプション)
            if let Some(ref expected_banner) = target.body_contains {
                if let Some(ref banner) = banner_opt {
                    if !banner.contains(expected_banner.as_str()) {
                        return CheckOutcome {
                            is_success: false,
                            status_code: None,
                            response_time_ms: elapsed,
                            error_message: Some(format!(
                                "Banner mismatch: expected '{}' not found",
                                expected_banner
                            )),
                            tls_expiry_date: None,
                            tls_days_left: None,
                            details: Some(format!("Banner: {}", &banner[..banner.len().min(200)])),
                        };
                    }
                } else {
                    return CheckOutcome::failure(
                        "No banner received but body_contains check required".to_string(),
                        elapsed,
                    );
                }
            }

            CheckOutcome {
                is_success: true,
                status_code: None,
                response_time_ms: elapsed,
                error_message: None,
                tls_expiry_date: None,
                tls_days_left: None,
                details: Some(format!("TCP connected to {} in {}ms", address, elapsed)),
            }
        }
        Err(e) => CheckOutcome::failure(
            format!("TCP connect failed to {}: {}", address, e),
            elapsed,
        ),
    }
}

/// Workers の connect() JS API を呼び出す TCP 接続ラッパー
///
/// `globalThis.connect(address, { secureTransport: "off" })` を使用
async fn tcp_connect_js(
    host: &str,
    port: u16,
    _timeout_sec: u32,
) -> std::result::Result<Option<String>, String> {
    use js_sys::{Object, Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let global = js_sys::global();

    // Workers の connect() グローバル関数を取得
    let connect_fn = Reflect::get(&global, &wasm_bindgen::JsValue::from_str("connect"))
        .map_err(|_| "connect() API not available".to_string())?;

    if connect_fn.is_undefined() {
        return Err("connect() API not available in this Workers environment".to_string());
    }

    let connect_fn: js_sys::Function = connect_fn
        .dyn_into()
        .map_err(|_| "connect is not a function".to_string())?;

    // 接続先アドレス
    let address = wasm_bindgen::JsValue::from_str(&format!("{}:{}", host, port));

    // オプション: secureTransport: "off" (プレーンTCP)
    let options = Object::new();
    Reflect::set(
        &options,
        &wasm_bindgen::JsValue::from_str("secureTransport"),
        &wasm_bindgen::JsValue::from_str("off"),
    )
    .map_err(|_| "Failed to set options".to_string())?;

    // connect() を呼び出し
    let promise = connect_fn
        .call2(&wasm_bindgen::JsValue::NULL, &address, &options)
        .map_err(|e| format!("connect() call failed: {:?}", e))?;

    let socket = JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("TCP connection error: {:?}", e))?;

    // readable ストリームからバナーを読み取り
    let readable = Reflect::get(&socket, &wasm_bindgen::JsValue::from_str("readable"))
        .map_err(|_| "No readable stream".to_string())?;

    let reader = js_sys::Reflect::get(&readable, &wasm_bindgen::JsValue::from_str("getReader"))
        .and_then(|get_reader_fn| {
            let f: js_sys::Function = get_reader_fn.dyn_into().map_err(|_| wasm_bindgen::JsValue::NULL)?;
            f.call0(&readable)
        })
        .map_err(|_| "Failed to get reader".to_string())?;

    // 最初のチャンクを読み取り (バナー)
    let read_fn = Reflect::get(&reader, &wasm_bindgen::JsValue::from_str("read"))
        .map_err(|_| "No read method".to_string())?;
    let read_fn: js_sys::Function = read_fn
        .dyn_into()
        .map_err(|_| "read is not a function".to_string())?;

    let read_promise = read_fn
        .call0(&reader)
        .map_err(|e| format!("read() failed: {:?}", e))?;

    let chunk_result = JsFuture::from(js_sys::Promise::from(read_promise))
        .await
        .map_err(|e| format!("Read error: {:?}", e))?;

    let done = Reflect::get(&chunk_result, &wasm_bindgen::JsValue::from_str("done"))
        .unwrap_or(wasm_bindgen::JsValue::TRUE);

    if done.is_truthy() {
        // ストリーム終了 (バナーなし) - 接続自体は成功
        return Ok(None);
    }

    let value = Reflect::get(&chunk_result, &wasm_bindgen::JsValue::from_str("value"))
        .map_err(|_| "No value in chunk".to_string())?;

    let array: Uint8Array = value
        .dyn_into()
        .map_err(|_| "Value is not Uint8Array".to_string())?;
    let bytes = array.to_vec();
    let banner = String::from_utf8_lossy(&bytes).to_string();

    // ソケットクローズ
    let close_fn = Reflect::get(&socket, &wasm_bindgen::JsValue::from_str("close"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(close) = close_fn {
        let _ = close.call0(&socket);
    }

    Ok(Some(banner))
}
