//! Email delivery via SMTP, powered by `wasm-smtp` + `wasm-smtp-cloudflare`.
//!
//! ## Status
//!
//! Email delivery is fully active. Active since 0.17.0. refreshed in 0.21.0
//! (wasm-smtp 0.9.3 with SCRAM-SHA-256), and refactored in 0.22.0 to use
//! `mail-builder` for RFC 5322 / MIME composition instead of the prior
//! hand-rolled implementation.
//!
//! ## Configuration
//!
//! The Core's `wrangler.toml` exposes the following env vars:
//!
//! - `EMAIL_SMTP_HOST` — empty disables email entirely
//! - `EMAIL_SMTP_PORT`
//! - `EMAIL_SMTP_USERNAME`
//! - `EMAIL_SMTP_TLS_MODE` — optional override; when unset, derived from port
//!   (465 → implicit TLS / SMTPS; anything else → STARTTLS)
//! - `EMAIL_FROM_ADDRESS`
//! - `EMAIL_FROM_NAME`
//!
//! and one secret:
//!
//! - `EMAIL_SMTP_PASSWORD`
//!
//! ## Why a single shared SMTP relay rather than per-channel credentials
//!
//! Per-channel SMTP credentials would let one Noye deployment send mail
//! through arbitrary mail relays, which sounds flexible but is operationally
//! expensive: every channel would need its own credential rotation, every
//! credential would have its own deliverability story (DKIM, SPF, return
//! path), and the schema would need a separate secret per channel. None of
//! that fits a small monitoring tool.
//!
//! The deployment-level relay matches how teams actually run alerting: one
//! identity for "this is from your monitoring system," many recipients.
//!
//! ## Why is_valid_email exists alongside the SMTP server's own checks
//!
//! The SMTP relay will reject malformed RCPT addresses anyway, but its error
//! reply is hard to map to "this specific channel has a typo." Catching it
//! before opening the connection keeps the operator-facing error tied to the
//! channel that owns the bad address.
//!
//! ## Why mail-builder
//!
//! Composing RFC 5322 messages by hand is solved-problem territory: line
//! folding, encoded-word selection (Q vs B), header injection defenses,
//! CRLF normalization. `mail-builder` (Stalwart Labs) does all of that and
//! has zero required dependencies. 0.22.0 removed roughly 80 lines of
//! Noye-side message-composition code in favor of the library.

use mail_builder::{MessageBuilder, headers::date::Date as MbDate};
use worker::*;

/// Selected TLS mode for the SMTP connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Connect with TLS already established (SMTPS, typically port 465).
    Implicit,
    /// Connect plaintext, EHLO, then upgrade to TLS via STARTTLS (typically port 587).
    StartTls,
}

impl TlsMode {
    /// Pick a mode based on the configured port: 465 → implicit, otherwise
    /// STARTTLS. This matches the long-standing convention across providers
    /// (Gmail / SES / SendGrid / Resend / Mailgun all use the same port-to-mode
    /// mapping).
    pub fn for_port(port: u16) -> Self {
        if port == 465 {
            Self::Implicit
        } else {
            Self::StartTls
        }
    }

    /// Parse an explicit override. Returns `None` for unrecognized values so
    /// the caller can fall back to [`Self::for_port`] rather than failing.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "implicit" | "smtps" | "tls" => Some(Self::Implicit),
            "starttls" => Some(Self::StartTls),
            _ => None,
        }
    }
}

/// SMTP configuration loaded from env. All fields are required for delivery
/// to actually happen; if `host` is empty the whole subsystem is treated as
/// "not configured" (a soft no-op that still passes type checks downstream).
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: String,
    pub tls_mode: TlsMode,
}

/// Result of inspecting the env for SMTP configuration.
pub enum ConfigStatus {
    /// `EMAIL_SMTP_HOST` is empty. Email is intentionally disabled.
    Disabled,
    /// `EMAIL_SMTP_HOST` is set but at least one other required value is
    /// missing or malformed. We surface a specific reason so the operator
    /// can fix it without guessing.
    Misconfigured(&'static str),
    /// All required values are present.
    Ok(SmtpConfig),
}

/// Inspect env for SMTP config. Pure-ish (depends on `Env` for var/secret
/// reads, but no I/O), so the gating logic is straightforward to reason
/// about.
pub fn load_config(env: &Env) -> ConfigStatus {
    let host = env
        .var("EMAIL_SMTP_HOST")
        .map(|v| v.to_string())
        .unwrap_or_default();
    if host.is_empty() {
        return ConfigStatus::Disabled;
    }

    let port = match env.var("EMAIL_SMTP_PORT").map(|v| v.to_string()) {
        Ok(s) => match s.parse::<u16>() {
            Ok(n) if n > 0 => n,
            _ => return ConfigStatus::Misconfigured("EMAIL_SMTP_PORT is not a valid port number"),
        },
        Err(_) => return ConfigStatus::Misconfigured("EMAIL_SMTP_PORT is not set"),
    };

    let username = match env.var("EMAIL_SMTP_USERNAME").map(|v| v.to_string()) {
        Ok(s) if !s.is_empty() => s,
        _ => return ConfigStatus::Misconfigured("EMAIL_SMTP_USERNAME is not set"),
    };

    // Read the password from secret first, falling back to var for local-dev
    // convenience.
    let password = match env.secret("EMAIL_SMTP_PASSWORD").map(|v| v.to_string()) {
        Ok(s) if !s.is_empty() => s,
        _ => match env.var("EMAIL_SMTP_PASSWORD").map(|v| v.to_string()) {
            Ok(s) if !s.is_empty() => s,
            _ => {
                return ConfigStatus::Misconfigured("EMAIL_SMTP_PASSWORD secret is not registered");
            }
        },
    };

    let from_address = match env.var("EMAIL_FROM_ADDRESS").map(|v| v.to_string()) {
        Ok(s) if is_valid_email(&s) => s,
        Ok(_) => return ConfigStatus::Misconfigured("EMAIL_FROM_ADDRESS is set but malformed"),
        Err(_) => return ConfigStatus::Misconfigured("EMAIL_FROM_ADDRESS is not set"),
    };

    let from_name = env
        .var("EMAIL_FROM_NAME")
        .map(|v| v.to_string())
        .unwrap_or_default();

    // TLS mode: explicit override wins, otherwise derive from the port.
    let tls_mode = env
        .var("EMAIL_SMTP_TLS_MODE")
        .ok()
        .and_then(|v| TlsMode::parse(&v.to_string()))
        .unwrap_or_else(|| TlsMode::for_port(port));

    ConfigStatus::Ok(SmtpConfig {
        host,
        port,
        username,
        password,
        from_address,
        from_name,
        tls_mode,
    })
}

/// Lightweight email-shape check. Not a full RFC 5322 parser — we only catch
/// typos that the SMTP server would reject anyway, but earlier and with a
/// channel-specific error message.
pub fn is_valid_email(s: &str) -> bool {
    if s.is_empty() || s.len() > 254 {
        return false;
    }
    let parts: Vec<&str> = s.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

/// Build a `MessageBuilder` for one notification email.
///
/// Pulled out as its own function so the construction is unit-testable —
/// we serialize the result via `write_to_string` in tests and assert on
/// the rendered headers, without ever touching SMTP.
///
/// The builder's lifetime is tied to all its `&str` inputs (`'a`); the
/// caller keeps them alive until `write_to_string` (or `send_message`)
/// finishes. For our use this is a single statement so the borrow
/// lifetimes are trivially correct.
pub fn build_message<'a>(
    from_name: &'a str,
    from_address: &'a str,
    to: &'a str,
    subject: &'a str,
    body: &'a str,
    message_id: String,
    now_unix: i64,
) -> MessageBuilder<'a> {
    // From-header: include the display name when it is non-empty, otherwise
    // pass just the address so the renderer doesn't emit an empty quoted
    // pair (`"" <addr>`). mail-builder canonicalizes either form correctly.
    let mut builder = MessageBuilder::new();
    builder = if from_name.trim().is_empty() {
        builder.from(from_address)
    } else {
        builder.from((from_name, from_address))
    };

    builder
        .to(to)
        .subject(subject)
        // `Date::new` takes a unix timestamp; passing it explicitly avoids
        // mail-builder's `Date::now()` which calls `SystemTime::now()` —
        // not always reliable on `wasm32-unknown-unknown`.
        .date(MbDate::new(now_unix))
        // Override mail-builder's auto-generated Message-ID path so the
        // domain matches the From-address. Many relays reject Message-IDs
        // whose domain doesn't match the sending domain.
        .message_id(message_id)
        // mail-builder picks Q- or B-encoding for the subject if it
        // contains non-ASCII; ASCII subjects pass through.
        .text_body(body)
        // Add a discoverability header so log auditors can see at a
        // glance which messages came from Noye.
        .header(
            "X-Mailer",
            mail_builder::headers::raw::Raw::new("noye-monitoring"),
        )
}

/// Send one email message via `wasm-smtp`.
///
/// The full SMTP conversation (CONNECT → EHLO → optional STARTTLS → AUTH →
/// MAIL FROM → RCPT TO → DATA → QUIT) is handled by `wasm-smtp`; this function
/// composes the message via `mail-builder` and translates errors.
///
/// `cfg` must already be validated (e.g. by [`load_config`] returning
/// [`ConfigStatus::Ok`]). `to` is checked one more time to catch typos before
/// burning a network roundtrip.
pub async fn send_email(cfg: &SmtpConfig, to: &str, subject: &str, body: &str) -> Result<()> {
    if !is_valid_email(to) {
        return Err(Error::RustError(format!(
            "Recipient address is malformed: {}",
            to
        )));
    }

    // EHLO/HELO domain. Use the From address's domain so the relay's
    // anti-spoofing logic sees a coherent identity.
    let ehlo_domain = cfg.from_address.split('@').nth(1).unwrap_or("noye.local");

    // Compose the message id with a domain that matches From — see the
    // commentary in `build_message`.
    let msgid_local = uuid::Uuid::new_v4().to_string();
    let msgid_domain = cfg.from_address.split('@').nth(1).unwrap_or("noye.local");
    let message_id = format!("{}@{}", msgid_local, msgid_domain);
    let now_unix = chrono::Utc::now().timestamp();

    let message = build_message(
        &cfg.from_name,
        &cfg.from_address,
        to,
        subject,
        body,
        message_id,
        now_unix,
    );

    let connect_result = match cfg.tls_mode {
        TlsMode::Implicit => {
            wasm_smtp_cloudflare::connect_smtps(&cfg.host, cfg.port, ehlo_domain).await
        }
        TlsMode::StartTls => {
            wasm_smtp_cloudflare::connect_smtp_starttls(&cfg.host, cfg.port, ehlo_domain).await
        }
    };
    let mut client = connect_result.map_err(map_smtp_error)?;

    client
        .login(&cfg.username, &cfg.password)
        .await
        .map_err(map_smtp_error)?;

    // `send_message` serializes the MessageBuilder via `write_to_string`
    // and forwards to `send_mail`. Any write error from mail-builder is
    // surfaced inside `SmtpError::Io` with the underlying source preserved.
    client
        .send_message(&cfg.from_address, &[to], message)
        .await
        .map_err(map_smtp_error)?;

    // Best-effort QUIT — if the server already closed the connection (e.g.
    // after a successful DATA), QUIT may fail. We don't surface that as a
    // delivery error because the message has already been accepted.
    let _ = client.quit().await;

    Ok(())
}

/// Translate a `wasm-smtp` error into a `worker::Error` that is safe to log
/// and to surface to the operator. The crate's `Display` impls already produce
/// useful, human-readable messages — we just wrap them.
fn map_smtp_error(err: wasm_smtp::SmtpError) -> Error {
    Error::RustError(format!("smtp send failed: {}", err))
}

/// User-facing diagnostic string for a config status. Used by the test-send
/// path so the operator gets a precise message rather than a generic error.
pub fn status_message(status: &ConfigStatus) -> String {
    match status {
        ConfigStatus::Disabled => {
            "Email delivery is not configured on this deployment. Set EMAIL_SMTP_HOST and the related env vars in crates/core/wrangler.toml, register EMAIL_SMTP_PASSWORD as a secret, and redeploy the Core.".to_string()
        }
        ConfigStatus::Misconfigured(why) => {
            format!("Email delivery is partially configured but cannot be used: {}", why)
        }
        ConfigStatus::Ok(_) => {
            "Email delivery is configured and active.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_valid_email ──

    #[test]
    fn valid_email_examples() {
        assert!(is_valid_email("ops@example.com"));
        assert!(is_valid_email("alerts+critical@sub.example.co.jp"));
    }

    #[test]
    fn invalid_email_examples() {
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("noatsign"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("ops@"));
        assert!(!is_valid_email("ops@example")); // no dot in domain
        assert!(!is_valid_email("a@b@example.com")); // multiple @
    }

    #[test]
    fn email_too_long_is_rejected() {
        let long = format!("{}@example.com", "a".repeat(300));
        assert!(!is_valid_email(&long));
    }

    // ── TlsMode ──

    #[test]
    fn tls_mode_for_port_picks_implicit_for_465() {
        assert_eq!(TlsMode::for_port(465), TlsMode::Implicit);
    }

    #[test]
    fn tls_mode_for_port_picks_starttls_for_others() {
        assert_eq!(TlsMode::for_port(587), TlsMode::StartTls);
        assert_eq!(TlsMode::for_port(25), TlsMode::StartTls);
        assert_eq!(TlsMode::for_port(2525), TlsMode::StartTls);
    }

    #[test]
    fn tls_mode_parse_recognizes_aliases() {
        assert_eq!(TlsMode::parse("implicit"), Some(TlsMode::Implicit));
        assert_eq!(TlsMode::parse("Implicit"), Some(TlsMode::Implicit));
        assert_eq!(TlsMode::parse("smtps"), Some(TlsMode::Implicit));
        assert_eq!(TlsMode::parse("tls"), Some(TlsMode::Implicit));
        assert_eq!(TlsMode::parse("starttls"), Some(TlsMode::StartTls));
        assert_eq!(TlsMode::parse("STARTTLS"), Some(TlsMode::StartTls));
    }

    #[test]
    fn tls_mode_parse_rejects_unknown() {
        assert_eq!(TlsMode::parse(""), None);
        assert_eq!(TlsMode::parse("none"), None);
        assert_eq!(TlsMode::parse("plain"), None);
    }

    // ── build_message ──
    //
    // We assert on the serialized output so any future drift in mail-builder's
    // header rendering is caught here rather than at the relay.

    fn render(builder: MessageBuilder<'_>) -> String {
        builder
            .write_to_string()
            .expect("write_to_string should not fail in tests")
    }

    #[test]
    fn build_message_includes_from_with_name() {
        let m = build_message(
            "Noye Monitoring",
            "noye@example.com",
            "ops@example.org",
            "Hello",
            "body text",
            "id123@example.com".to_string(),
            1_777_903_380, // 2026-05-04 14:03:00 UTC
        );
        let s = render(m);
        // The address must be present; the display name should also appear
        // (exact quoting style is mail-builder's contract, which we don't
        // pin here — we just need to know the renderer preserved both).
        assert!(s.contains("noye@example.com"), "From address missing: {s}");
        assert!(s.contains("Noye Monitoring"), "From name missing: {s}");
    }

    #[test]
    fn build_message_omits_empty_from_name() {
        let m = build_message(
            "",
            "noye@example.com",
            "ops@example.org",
            "Hello",
            "body",
            "id@d".to_string(),
            0,
        );
        let s = render(m);
        // Without a display name the From header is just the address.
        // We assert the absence of the `"" <...>` quoted-empty form
        // (defensive: mail-builder sometimes renders this for empty strings).
        assert!(!s.contains(r#""" <"#), "empty quoted-pair leaked: {s}");
        assert!(s.contains("noye@example.com"));
    }

    #[test]
    fn build_message_includes_to_subject_message_id_date() {
        let m = build_message(
            "N",
            "n@example.com",
            "ops@example.org",
            "[TEST] hello",
            "body",
            "uniq-msgid@example.com".to_string(),
            1_777_903_380,
        );
        let s = render(m);
        assert!(s.contains("To:"), "To header missing");
        assert!(s.contains("ops@example.org"));
        assert!(s.contains("Subject:"));
        assert!(s.contains("[TEST] hello"));
        assert!(s.contains("Message-ID:"));
        assert!(
            s.contains("<uniq-msgid@example.com>"),
            "explicit Message-ID not used: {s}"
        );
        assert!(s.contains("Date:"));
    }

    #[test]
    fn build_message_includes_x_mailer() {
        let m = build_message(
            "N",
            "n@example.com",
            "ops@example.org",
            "s",
            "b",
            "id@d".to_string(),
            0,
        );
        let s = render(m);
        assert!(
            s.contains("X-Mailer: noye-monitoring"),
            "X-Mailer missing: {s}"
        );
    }

    #[test]
    fn build_message_separates_headers_from_body_with_crlf_blank_line() {
        let m = build_message(
            "N",
            "n@d.com",
            "to@x.com",
            "s",
            "BODY",
            "id@d".to_string(),
            0,
        );
        let s = render(m);
        // RFC 5322 §2.1: headers and body are separated by an empty line
        // (CRLF CRLF). We just check the marker appears and that "BODY"
        // sits after it.
        let split = s.find("\r\n\r\n").expect("blank-line separator missing");
        let (_headers, body_section) = s.split_at(split + 4);
        assert!(body_section.contains("BODY"));
    }

    #[test]
    fn build_message_text_body_passes_through() {
        let m = build_message(
            "N",
            "n@d.com",
            "to@x.com",
            "s",
            "line one\nline two",
            "id@d".to_string(),
            0,
        );
        let s = render(m);
        // mail-builder normalizes line endings to CRLF in the rendered
        // output. The text content is preserved either with quoted-printable
        // or 8bit encoding — either way both lines should appear.
        assert!(s.contains("line one"), "first line of body missing: {s}");
        assert!(s.contains("line two"), "second line of body missing: {s}");
    }

    #[test]
    fn build_message_subject_with_non_ascii_uses_encoded_word() {
        let m = build_message(
            "N",
            "n@d.com",
            "to@x.com",
            "Café down",
            "body",
            "id@d".to_string(),
            0,
        );
        let s = render(m);
        // mail-builder may pick Q-encoding or B-encoding for non-ASCII;
        // we assert only the encoded-word framing is present.
        assert!(
            s.contains("=?utf-8?B?") || s.contains("=?utf-8?Q?"),
            "non-ASCII subject was not encoded: {s}"
        );
    }

    // ── status_message ──

    #[test]
    fn status_message_disabled_mentions_smtp_host() {
        let msg = status_message(&ConfigStatus::Disabled);
        assert!(msg.contains("EMAIL_SMTP_HOST"));
    }

    #[test]
    fn status_message_misconfigured_includes_reason() {
        let msg = status_message(&ConfigStatus::Misconfigured("EMAIL_SMTP_PORT is not set"));
        assert!(msg.contains("EMAIL_SMTP_PORT is not set"));
    }
}
