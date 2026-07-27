//! In-memory store of pending authorization codes.
//!
//! When `/authorize` is hit, the IdP generates a `code` and stashes the
//! associated `state`, `nonce`, and `code_challenge` here. When `/token`
//! is hit, the same `code` is consumed (one-shot) and the stored values
//! drive ID Token construction.
//!
//! Codes expire after 60 seconds. We do not garbage-collect background-
//! ly — the next consume_code call removes expired entries opportunistic-
//! ally. For a dev tool with at most a few codes outstanding, this is
//! plenty.

use std::collections::HashMap;
use std::sync::Mutex;

const CODE_LIFETIME_SECONDS: i64 = 60;

#[derive(Clone, Debug)]
pub struct PendingCode {
    pub state: String,
    pub nonce: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub redirect_uri: String,
    pub created_at: i64,
}

pub struct CodeStore {
    inner: Mutex<HashMap<String, PendingCode>>,
}

impl CodeStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn put(&self, code: String, pending: PendingCode) {
        let mut guard = self.inner.lock().expect("CodeStore mutex poisoned");
        // Opportunistic GC of expired entries.
        let now = chrono::Utc::now().timestamp();
        guard.retain(|_, p| now - p.created_at < CODE_LIFETIME_SECONDS);
        guard.insert(code, pending);
    }

    /// Remove and return the entry for `code` if it exists and is unexpired.
    pub fn consume(&self, code: &str) -> Option<PendingCode> {
        let mut guard = self.inner.lock().expect("CodeStore mutex poisoned");
        let now = chrono::Utc::now().timestamp();
        if let Some(p) = guard.remove(code) {
            if now - p.created_at < CODE_LIFETIME_SECONDS {
                return Some(p);
            }
        }
        None
    }
}

impl Default for CodeStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(state: &str) -> PendingCode {
        PendingCode {
            state: state.to_string(),
            nonce: "n".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            redirect_uri: "http://localhost:8787/auth/callback".to_string(),
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    #[test]
    fn put_then_consume_returns_value_once() {
        let store = CodeStore::new();
        store.put("c1".to_string(), pending("s1"));
        let got = store.consume("c1").unwrap();
        assert_eq!(got.state, "s1");
        // Second consume returns None
        assert!(store.consume("c1").is_none());
    }

    #[test]
    fn consume_unknown_returns_none() {
        let store = CodeStore::new();
        assert!(store.consume("ghost").is_none());
    }

    #[test]
    fn consume_expired_returns_none() {
        let store = CodeStore::new();
        let mut p = pending("s1");
        p.created_at = chrono::Utc::now().timestamp() - 120;
        store.put("c1".to_string(), p);
        assert!(store.consume("c1").is_none());
    }
}
