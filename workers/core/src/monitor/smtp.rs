use worker::*;

use noye_shared::Target;
use super::CheckOutcome;

/// SMTP ヘルスチェック (要件2-3)
///
/// 検証項目:
/// - ポート接続成功
/// - サーバーバナー受信 (220 グリーティング)
/// - EHLO/HELO 応答成功
/// - STARTTLS 開始可否の確認
///
/// Workers の connect() API による TCP ソケットでSMTPプロトコルを手動実行。
pub async fn check(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;

    let port = target.port.unwrap_or(25) as u16;
    let host = &target.host;

    match smtp_handshake(host, port, target.timeout_sec as u32).await {
        Ok(details) => {
            let elapsed = (js_sys::Date::now() as i64) - start;

            // タイムアウト判定
            let timeout_ms = (target.timeout_sec * 1000) as i64;
            if elapsed > timeout_ms {
                return CheckOutcome::failure(
                    format!("SMTP timeout: {}ms > {}ms limit", elapsed, timeout_ms),
                    elapsed,
                );
            }

            CheckOutcome {
                is_success: true,
                status_code: Some(220), // SMTP banner code
                response_time_ms: elapsed,
                error_message: None,
                tls_expiry_date: None,
                tls_days_left: None,
                details: Some(details),
            }
        }
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            CheckOutcome::failure(format!("SMTP check failed: {}", e), elapsed)
        }
    }
}

/// SMTP ハンドシェイクの各ステップを実行する
async fn smtp_handshake(
    host: &str,
    port: u16,
    _timeout_sec: u32,
) -> std::result::Result<String, String> {
    use js_sys::{Object, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let global = js_sys::global();

    let connect_fn = Reflect::get(&global, &wasm_bindgen::JsValue::from_str("connect"))
        .map_err(|_| "connect() API not available".to_string())?;

    if connect_fn.is_undefined() {
        return Err("connect() API not available".to_string());
    }

    let connect_fn: js_sys::Function = connect_fn
        .dyn_into()
        .map_err(|_| "connect is not a function".to_string())?;

    let address = wasm_bindgen::JsValue::from_str(&format!("{}:{}", host, port));
    let options = Object::new();
    Reflect::set(
        &options,
        &wasm_bindgen::JsValue::from_str("secureTransport"),
        &wasm_bindgen::JsValue::from_str("off"),
    )
    .map_err(|_| "Failed to set options".to_string())?;

    let promise = connect_fn
        .call2(&wasm_bindgen::JsValue::NULL, &address, &options)
        .map_err(|e| format!("connect() failed: {:?}", e))?;

    let socket = JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("SMTP connection error: {:?}", e))?;

    let mut details = Vec::new();

    // Step 1: バナー受信 (220 グリーティング)
    let banner = read_line_from_socket(&socket).await?;
    if !banner.starts_with("220") {
        return Err(format!("Expected 220 greeting, got: {}", banner.trim()));
    }
    details.push(format!("Banner: {}", banner.trim()));

    // Step 2: EHLO 送信
    write_to_socket(&socket, &format!("EHLO {}\r\n", host)).await?;
    let ehlo_response = read_line_from_socket(&socket).await?;
    if !ehlo_response.starts_with("250") {
        // EHLO 失敗時は HELO にフォールバック
        write_to_socket(&socket, &format!("HELO {}\r\n", host)).await?;
        let helo_response = read_line_from_socket(&socket).await?;
        if !helo_response.starts_with("250") {
            return Err(format!(
                "EHLO/HELO failed: EHLO={}, HELO={}",
                ehlo_response.trim(),
                helo_response.trim()
            ));
        }
        details.push(format!("HELO: {}", helo_response.trim()));
    } else {
        details.push(format!("EHLO: {}", ehlo_response.trim()));
    }

    // Step 3: STARTTLS 可否の確認
    let supports_starttls = ehlo_response.contains("STARTTLS");
    if supports_starttls {
        write_to_socket(&socket, "STARTTLS\r\n").await?;
        let tls_response = read_line_from_socket(&socket).await?;
        if tls_response.starts_with("220") {
            details.push("STARTTLS: supported and ready".to_string());
        } else {
            details.push(format!("STARTTLS: response {}", tls_response.trim()));
        }
    } else {
        details.push("STARTTLS: not advertised".to_string());
    }

    // QUIT
    let _ = write_to_socket(&socket, "QUIT\r\n").await;

    // クローズ
    let close_fn = Reflect::get(&socket, &wasm_bindgen::JsValue::from_str("close"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(close) = close_fn {
        let _ = close.call0(&socket);
    }

    Ok(details.join("; "))
}

/// ソケットから1行を読み取る
async fn read_line_from_socket(
    socket: &wasm_bindgen::JsValue,
) -> std::result::Result<String, String> {
    use js_sys::{Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let readable = Reflect::get(socket, &wasm_bindgen::JsValue::from_str("readable"))
        .map_err(|_| "No readable stream".to_string())?;

    let get_reader_fn = Reflect::get(&readable, &wasm_bindgen::JsValue::from_str("getReader"))
        .map_err(|_| "No getReader".to_string())?;
    let get_reader_fn: js_sys::Function = get_reader_fn
        .dyn_into()
        .map_err(|_| "getReader is not a function".to_string())?;
    let reader = get_reader_fn
        .call0(&readable)
        .map_err(|e| format!("getReader() failed: {:?}", e))?;

    let read_fn = Reflect::get(&reader, &wasm_bindgen::JsValue::from_str("read"))
        .map_err(|_| "No read method".to_string())?;
    let read_fn: js_sys::Function = read_fn
        .dyn_into()
        .map_err(|_| "read is not a function".to_string())?;

    let promise = read_fn
        .call0(&reader)
        .map_err(|e| format!("read() failed: {:?}", e))?;

    let result = JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("Read error: {:?}", e))?;

    let done = Reflect::get(&result, &wasm_bindgen::JsValue::from_str("done"))
        .unwrap_or(wasm_bindgen::JsValue::TRUE);

    if done.is_truthy() {
        return Err("Stream ended unexpectedly".to_string());
    }

    let value = Reflect::get(&result, &wasm_bindgen::JsValue::from_str("value"))
        .map_err(|_| "No value".to_string())?;

    let array: Uint8Array = value
        .dyn_into()
        .map_err(|_| "Value is not Uint8Array".to_string())?;

    // reader をリリース
    let release_fn = Reflect::get(&reader, &wasm_bindgen::JsValue::from_str("releaseLock"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(release) = release_fn {
        let _ = release.call0(&reader);
    }

    Ok(String::from_utf8_lossy(&array.to_vec()).to_string())
}

/// ソケットにデータを書き込む
async fn write_to_socket(
    socket: &wasm_bindgen::JsValue,
    data: &str,
) -> std::result::Result<(), String> {
    use js_sys::{Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let writable = Reflect::get(socket, &wasm_bindgen::JsValue::from_str("writable"))
        .map_err(|_| "No writable stream".to_string())?;

    let get_writer_fn = Reflect::get(&writable, &wasm_bindgen::JsValue::from_str("getWriter"))
        .map_err(|_| "No getWriter".to_string())?;
    let get_writer_fn: js_sys::Function = get_writer_fn
        .dyn_into()
        .map_err(|_| "getWriter is not a function".to_string())?;
    let writer = get_writer_fn
        .call0(&writable)
        .map_err(|e| format!("getWriter() failed: {:?}", e))?;

    let bytes = data.as_bytes();
    let array = Uint8Array::new_with_length(bytes.len() as u32);
    array.copy_from(bytes);

    let write_fn = Reflect::get(&writer, &wasm_bindgen::JsValue::from_str("write"))
        .map_err(|_| "No write method".to_string())?;
    let write_fn: js_sys::Function = write_fn
        .dyn_into()
        .map_err(|_| "write is not a function".to_string())?;

    let promise = write_fn
        .call1(&writer, &array)
        .map_err(|e| format!("write() failed: {:?}", e))?;

    JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| format!("Write error: {:?}", e))?;

    // writer をリリース
    let release_fn = Reflect::get(&writer, &wasm_bindgen::JsValue::from_str("releaseLock"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(release) = release_fn {
        let _ = release.call0(&writer);
    }

    Ok(())
}
