use worker::*;

/// 指定した Cookie の値をリクエストヘッダから取り出す。
///
/// Set-Cookie ではなく Cookie ヘッダ (クライアント→サーバ方向) を対象とする。
/// 複数 Cookie が `;` 区切りで並ぶ RFC 6265 形式を前提にパースする。
pub fn get(req: &Request, name: &str) -> Result<Option<String>> {
    let cookie_header = match req.headers().get("Cookie")? {
        Some(v) => v,
        None => return Ok(None),
    };

    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Ok(Some(v.trim().to_string()));
            }
        }
    }
    Ok(None)
}

/// Set-Cookie ヘッダ値を組み立てる。
///
/// デフォルトで最も安全な設定 (HttpOnly + Secure + SameSite=Lax + Path=/) を適用する。
/// OIDC コールバックを同一サイトから受けるため SameSite=Strict ではなく Lax。
pub struct CookieBuilder {
    name: String,
    value: String,
    max_age_sec: Option<i64>,
    path: String,
    same_site: String,
    http_only: bool,
    secure: bool,
}

impl CookieBuilder {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            max_age_sec: None,
            path: "/".to_string(),
            same_site: "Lax".to_string(),
            http_only: true,
            secure: true,
        }
    }

    pub fn max_age(mut self, seconds: i64) -> Self {
        self.max_age_sec = Some(seconds);
        self
    }

    /// ログアウト用: 即時失効 Cookie (Max-Age=0)
    pub fn expired(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: String::new(),
            max_age_sec: Some(0),
            path: "/".to_string(),
            same_site: "Lax".to_string(),
            http_only: true,
            secure: true,
        }
    }

    pub fn build(self) -> String {
        let mut parts = vec![format!("{}={}", self.name, self.value)];
        parts.push(format!("Path={}", self.path));
        parts.push(format!("SameSite={}", self.same_site));
        if self.http_only {
            parts.push("HttpOnly".to_string());
        }
        if self.secure {
            parts.push("Secure".to_string());
        }
        if let Some(sec) = self.max_age_sec {
            parts.push(format!("Max-Age={}", sec));
        }
        parts.join("; ")
    }
}
