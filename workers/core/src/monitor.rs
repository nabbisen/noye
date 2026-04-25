pub mod engine;
pub mod http;
pub mod smtp;
pub mod tcp;
pub mod tls;

use serde::{Deserialize, Serialize};

/// ヘルスチェック結果の共通構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub is_success: bool,
    pub status_code: Option<i64>,
    pub response_time_ms: i64,
    pub error_message: Option<String>,
    pub tls_expiry_date: Option<String>,
    pub tls_days_left: Option<i64>,
    pub details: Option<String>,
}

impl CheckOutcome {
    pub fn failure(error: String, response_time_ms: i64) -> Self {
        Self {
            is_success: false,
            status_code: None,
            response_time_ms,
            error_message: Some(error),
            tls_expiry_date: None,
            tls_days_left: None,
            details: None,
        }
    }
}
