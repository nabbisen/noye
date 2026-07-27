//! Rate limits applied at the Gateway edge.
//!
//! Two distinct rate-limit families share this module's pure-logic primitives:
//!
//! 1. **Per-channel test-send limit** (`check_and_consume`). Default 5/min and
//!    30/hour. Scoped to the notification channel; counted *after*
//!    authentication. Purpose: bound the accidental-blast radius for the
//!    operator-facing "send test notification" button.
//!
//! 2. **Per-IP login limit** (`check_and_consume_login`). Default 10/min and
//!    50/hour. Scoped to the client IP from the `CF-Connecting-IP` header;
//!    counted *before* authentication. Purpose: prevent unauthenticated
//!    attackers from filling KV with `pending_login:` entries (a DoS / KV
//!    quota-exhaustion vector) and from brute-forcing OIDC state replay.
//!
//! Both families use the same fixed-window counter pattern over Cloudflare
//! KV: two simultaneous windows (per-minute and per-hour), independent
//! checks, both must pass.
//!
//! ## Why fixed-window
//!
//! Sliding-window or token-bucket would be more accurate but require either a
//! Durable Object or read-then-write atomicity that KV does not offer. For
//! both use cases, fixed-window is good enough: the worst case is a single
//! burst at a window boundary, which is bounded by `2 * limit` and still well
//! below any external provider's protection threshold.
//!
//! ## Why on the Gateway, not the Core
//!
//! Counting before the Service Binding hop saves a roundtrip and ensures
//! abusive traffic never crosses the trust boundary.

use worker::*;

const KV_BINDING: &str = "CACHE_KV";
const TEST_PREFIX: &str = "ratelimit:test:";
const LOGIN_PREFIX: &str = "ratelimit:login:";

/// Default safe limits when env vars are not configured.
const DEFAULT_TEST_PER_MIN: u32 = 5;
const DEFAULT_TEST_PER_HOUR: u32 = 30;

/// Login limits. Tighter than test-send because the requests are unauthenticated:
/// a browser-driven user logs in once or twice a day, so a 10/min / 50/hour cap
/// is far above any legitimate usage while still capping a single attacker IP
/// to small numbers per window.
const DEFAULT_LOGIN_PER_MIN: u32 = 10;
const DEFAULT_LOGIN_PER_HOUR: u32 = 50;

/// TTLs are slightly longer than the window itself so overlapping requests
/// near a boundary don't lose their counter early.
const MIN_TTL_SEC: u64 = 90; // 1.5 minutes
const HOUR_TTL_SEC: u64 = 4500; // 75 minutes

/// Outcome of a rate-limit check. Returned by [`check_and_consume`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The request is allowed. Counters have already been incremented.
    Allowed,
    /// The request is denied. `retry_after_sec` is a conservative estimate of
    /// how long the caller should wait before trying again (always at least 1).
    Denied {
        scope: Scope,
        retry_after_sec: u64,
    },
}

/// Which window triggered the denial. Used by the caller to format a useful
/// error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    PerMinute,
    PerHour,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::PerMinute => "per-minute",
            Scope::PerHour => "per-hour",
        }
    }
}

/// Compose the KV key for a (subject, scope, bucket-id) triplet.
///
/// `subject` is the channel ID (for test-send limits) or the IP address
/// (for login limits). `bucket_id` is the deterministic time-bucket label
/// (e.g. `"202604271403"` for the 14:03 minute window).
///
/// Pulled out of the I/O code so the key shape is covered by unit tests.
pub fn key_for(prefix: &str, subject: &str, scope: &Scope, bucket_id: &str) -> String {
    let scope_char = match scope {
        Scope::PerMinute => "m",
        Scope::PerHour => "h",
    };
    format!("{}{}:{}:{}", prefix, subject, scope_char, bucket_id)
}

/// Sanitize a raw IP address for use as part of a KV key.
///
/// Replaces `:` with `_` so an IPv6 address like `2001:db8::1` does not
/// collide with the `:` separators we use elsewhere in the key shape. The
/// substitution is reversible via simple replace (no other chars are
/// touched), but we never need to invert it — keys are write/read only.
///
/// Pure helper so its behavior is unit-tested.
pub fn ip_for_key(ip: &str) -> String {
    ip.replace(':', "_")
}

/// Build the deterministic bucket id for a given time and scope.
///
/// The format is intentionally collision-free across scopes:
/// - Minute window: `YYYYMMDDHHMM`
/// - Hour window: `YYYYMMDDHH`
pub fn bucket_id_for(now: chrono::DateTime<chrono::Utc>, scope: &Scope) -> String {
    match scope {
        Scope::PerMinute => now.format("%Y%m%d%H%M").to_string(),
        Scope::PerHour => now.format("%Y%m%d%H").to_string(),
    }
}

/// Pure decision step: given the *current* counters and configured limits,
/// decide whether to allow or deny. Extracted from [`check_and_consume`] so
/// the rule logic is unit-testable without KV.
///
/// `now` is used to compute the conservative `retry_after_sec` (seconds left
/// in the offending window). When both limits would fire, the more
/// restrictive (longer) wait is reported.
pub fn decide(
    now: chrono::DateTime<chrono::Utc>,
    minute_count: u32,
    hour_count: u32,
    per_min: u32,
    per_hour: u32,
) -> Decision {
    if minute_count >= per_min {
        return Decision::Denied {
            scope: Scope::PerMinute,
            retry_after_sec: seconds_until_next_minute(now),
        };
    }
    if hour_count >= per_hour {
        return Decision::Denied {
            scope: Scope::PerHour,
            retry_after_sec: seconds_until_next_hour(now),
        };
    }
    Decision::Allowed
}

/// Read configured test-send limits from env, falling back to safe defaults.
///
/// A misconfigured value (non-numeric or zero) silently falls back rather
/// than failing the request — operators should never be locked out of the
/// test action because of a typo in `wrangler.toml`.
fn test_limits(env: &Env) -> (u32, u32) {
    read_limits(
        env,
        "TEST_SEND_LIMIT_PER_MIN",
        "TEST_SEND_LIMIT_PER_HOUR",
        DEFAULT_TEST_PER_MIN,
        DEFAULT_TEST_PER_HOUR,
    )
}

/// Read configured login limits from env, falling back to safe defaults.
///
/// Same fallback semantics as `test_limits`. Note that for the login limit
/// we err on the side of accepting traffic when the operator misconfigures
/// the value — the alternative (locking everyone out) is much worse than
/// briefly running with looser limits.
fn login_limits(env: &Env) -> (u32, u32) {
    read_limits(
        env,
        "LOGIN_RATE_LIMIT_PER_MIN",
        "LOGIN_RATE_LIMIT_PER_HOUR",
        DEFAULT_LOGIN_PER_MIN,
        DEFAULT_LOGIN_PER_HOUR,
    )
}

fn read_limits(
    env: &Env,
    var_min: &str,
    var_hour: &str,
    default_min: u32,
    default_hour: u32,
) -> (u32, u32) {
    let per_min = env
        .var(var_min)
        .ok()
        .and_then(|v| v.to_string().parse::<u32>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default_min);
    let per_hour = env
        .var(var_hour)
        .ok()
        .and_then(|v| v.to_string().parse::<u32>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default_hour);
    (per_min, per_hour)
}

/// Run the full rate-limit check + consume cycle for a channel test-send.
///
/// On success (`Decision::Allowed`) both counters have been incremented. On
/// denial neither counter is touched, so the user can retry without burning
/// quota.
pub async fn check_and_consume(env: &Env, channel_id: &str) -> Result<Decision> {
    let kv = env.kv(KV_BINDING)?;
    let now = chrono::Utc::now();
    let (per_min, per_hour) = test_limits(env);

    let min_key = key_for(TEST_PREFIX, channel_id, &Scope::PerMinute, &bucket_id_for(now, &Scope::PerMinute));
    let hour_key = key_for(TEST_PREFIX, channel_id, &Scope::PerHour, &bucket_id_for(now, &Scope::PerHour));

    let minute_count = read_counter(&kv, &min_key).await;
    let hour_count = read_counter(&kv, &hour_key).await;

    let decision = decide(now, minute_count, hour_count, per_min, per_hour);

    if matches!(decision, Decision::Allowed) {
        // Best-effort increment; if KV write fails we still allow the request
        // (failing closed would lock out operators on transient KV outages).
        let _ = write_counter(&kv, &min_key, minute_count + 1, MIN_TTL_SEC).await;
        let _ = write_counter(&kv, &hour_key, hour_count + 1, HOUR_TTL_SEC).await;
    }

    Ok(decision)
}

/// Run the full rate-limit check + consume cycle for an unauthenticated
/// `/auth/login` request, scoped by client IP.
///
/// `client_ip` should be the value of the `CF-Connecting-IP` header at
/// the gateway. When that header is missing (e.g. `wrangler dev` from a
/// terminal that bypasses the Cloudflare edge), pass `"unknown"` —
/// every "unknown" caller then shares a single bucket, which is the
/// correct safe behavior.
///
/// Same semantics as the test-send variant: counters increment only on
/// the allow path; KV write failures fail open rather than lock users out.
pub async fn check_and_consume_login(env: &Env, client_ip: &str) -> Result<Decision> {
    let kv = env.kv(KV_BINDING)?;
    let now = chrono::Utc::now();
    let (per_min, per_hour) = login_limits(env);

    let subject = ip_for_key(client_ip);
    let min_key = key_for(LOGIN_PREFIX, &subject, &Scope::PerMinute, &bucket_id_for(now, &Scope::PerMinute));
    let hour_key = key_for(LOGIN_PREFIX, &subject, &Scope::PerHour, &bucket_id_for(now, &Scope::PerHour));

    let minute_count = read_counter(&kv, &min_key).await;
    let hour_count = read_counter(&kv, &hour_key).await;

    let decision = decide(now, minute_count, hour_count, per_min, per_hour);

    if matches!(decision, Decision::Allowed) {
        let _ = write_counter(&kv, &min_key, minute_count + 1, MIN_TTL_SEC).await;
        let _ = write_counter(&kv, &hour_key, hour_count + 1, HOUR_TTL_SEC).await;
    }

    Ok(decision)
}

async fn read_counter(kv: &kv::KvStore, key: &str) -> u32 {
    match kv.get(key).text().await {
        Ok(Some(s)) => s.parse::<u32>().unwrap_or(0),
        _ => 0,
    }
}

async fn write_counter(
    kv: &kv::KvStore,
    key: &str,
    value: u32,
    ttl_sec: u64,
) -> Result<()> {
    kv.put(key, value.to_string())?
        .expiration_ttl(ttl_sec)
        .execute()
        .await?;
    Ok(())
}

fn seconds_until_next_minute(now: chrono::DateTime<chrono::Utc>) -> u64 {
    let secs = 60_u64.saturating_sub(now.timestamp() as u64 % 60);
    secs.max(1)
}

fn seconds_until_next_hour(now: chrono::DateTime<chrono::Utc>) -> u64 {
    let secs = 3600_u64.saturating_sub(now.timestamp() as u64 % 3600);
    secs.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
    }

    // ── decide() ──

    #[test]
    fn allows_when_both_counters_below_limit() {
        let d = decide(at(2026, 4, 27, 12, 30, 15), 0, 0, 5, 30);
        assert_eq!(d, Decision::Allowed);

        let d = decide(at(2026, 4, 27, 12, 30, 15), 4, 29, 5, 30);
        assert_eq!(d, Decision::Allowed);
    }

    #[test]
    fn denies_when_minute_counter_at_limit() {
        let now = at(2026, 4, 27, 12, 30, 15);
        let d = decide(now, 5, 0, 5, 30);
        match d {
            Decision::Denied { scope, retry_after_sec } => {
                assert_eq!(scope, Scope::PerMinute);
                assert_eq!(retry_after_sec, 45); // 60 - 15 = 45 seconds left in the minute
            }
            _ => panic!("expected denial"),
        }
    }

    #[test]
    fn denies_when_hour_counter_at_limit_even_if_minute_is_fine() {
        let now = at(2026, 4, 27, 12, 30, 15);
        let d = decide(now, 0, 30, 5, 30);
        match d {
            Decision::Denied { scope, retry_after_sec } => {
                assert_eq!(scope, Scope::PerHour);
                // 3600 - (30*60 + 15) = 3600 - 1815 = 1785
                assert_eq!(retry_after_sec, 1785);
            }
            _ => panic!("expected denial"),
        }
    }

    #[test]
    fn minute_denial_takes_precedence_over_hour_denial() {
        // When both fire, surface the more immediate problem so the user
        // sees the shorter wait first.
        let now = at(2026, 4, 27, 12, 30, 0);
        let d = decide(now, 5, 30, 5, 30);
        match d {
            Decision::Denied { scope, .. } => assert_eq!(scope, Scope::PerMinute),
            _ => panic!("expected denial"),
        }
    }

    #[test]
    fn allows_when_counters_exactly_below_limit() {
        // Off-by-one guard
        let d = decide(at(2026, 4, 27, 0, 0, 0), 4, 29, 5, 30);
        assert_eq!(d, Decision::Allowed);
    }

    #[test]
    fn denies_when_counter_exceeds_limit() {
        // Should never happen in practice but defensive
        let d = decide(at(2026, 4, 27, 0, 0, 0), 100, 0, 5, 30);
        assert!(matches!(d, Decision::Denied { scope: Scope::PerMinute, .. }));
    }

    // ── retry_after edge cases ──

    #[test]
    fn retry_after_at_minute_boundary_is_at_least_one() {
        // At exactly the start of a minute, the counter rolls over immediately
        // — but we still tell the client to wait at least 1 second to avoid a
        // tight retry loop.
        let now = at(2026, 4, 27, 12, 30, 0);
        let d = decide(now, 5, 0, 5, 30);
        if let Decision::Denied { retry_after_sec, .. } = d {
            assert!(retry_after_sec >= 1);
            assert_eq!(retry_after_sec, 60);
        } else {
            panic!("expected denial");
        }
    }

    #[test]
    fn retry_after_just_before_minute_boundary_is_one() {
        // 59 seconds into the minute means 1 second until the next minute.
        let now = at(2026, 4, 27, 12, 30, 59);
        let d = decide(now, 5, 0, 5, 30);
        if let Decision::Denied { retry_after_sec, .. } = d {
            assert_eq!(retry_after_sec, 1);
        } else {
            panic!("expected denial");
        }
    }

    // ── key composition ──

    #[test]
    fn key_for_includes_scope_and_bucket_id() {
        let k = key_for(TEST_PREFIX, "ch-123", &Scope::PerMinute, "202604271203");
        assert_eq!(k, "ratelimit:test:ch-123:m:202604271203");

        let k = key_for(TEST_PREFIX, "ch-123", &Scope::PerHour, "2026042712");
        assert_eq!(k, "ratelimit:test:ch-123:h:2026042712");
    }

    #[test]
    fn key_for_isolates_distinct_channels() {
        let a = key_for(TEST_PREFIX, "ch-A", &Scope::PerMinute, "202604271203");
        let b = key_for(TEST_PREFIX, "ch-B", &Scope::PerMinute, "202604271203");
        assert_ne!(a, b);
    }

    #[test]
    fn key_for_isolates_distinct_scopes() {
        // Same channel, same bucket id, different scope -> distinct keys.
        let m = key_for(TEST_PREFIX, "ch-1", &Scope::PerMinute, "2026042712");
        let h = key_for(TEST_PREFIX, "ch-1", &Scope::PerHour, "2026042712");
        assert_ne!(m, h);
    }

    #[test]
    fn key_for_isolates_test_and_login_namespaces() {
        // Same subject, same bucket, but different prefix -> distinct keys.
        // This guards against accidentally sharing counters between the two
        // rate-limit families.
        let test = key_for(TEST_PREFIX, "1.2.3.4", &Scope::PerMinute, "202604271203");
        let login = key_for(LOGIN_PREFIX, "1.2.3.4", &Scope::PerMinute, "202604271203");
        assert_ne!(test, login);
        assert!(test.starts_with("ratelimit:test:"));
        assert!(login.starts_with("ratelimit:login:"));
    }

    #[test]
    fn key_for_login_with_ipv4() {
        let k = key_for(LOGIN_PREFIX, "203.0.113.5", &Scope::PerMinute, "202604271203");
        assert_eq!(k, "ratelimit:login:203.0.113.5:m:202604271203");
    }

    // ── ip_for_key() ──

    #[test]
    fn ip_for_key_passes_ipv4_unchanged() {
        assert_eq!(ip_for_key("203.0.113.5"), "203.0.113.5");
    }

    #[test]
    fn ip_for_key_replaces_colons_in_ipv6() {
        // RFC 5952 canonical IPv6 with embedded `::`
        assert_eq!(ip_for_key("2001:db8::1"), "2001_db8__1");
        assert_eq!(
            ip_for_key("fe80::1ff:fe23:4567:890a"),
            "fe80__1ff_fe23_4567_890a"
        );
    }

    #[test]
    fn ip_for_key_passes_unknown_unchanged() {
        // The fallback used when CF-Connecting-IP is missing.
        assert_eq!(ip_for_key("unknown"), "unknown");
    }

    #[test]
    fn ip_for_key_distinguishes_distinct_ipv6_addresses() {
        let a = ip_for_key("2001:db8::1");
        let b = ip_for_key("2001:db8::2");
        assert_ne!(a, b);
    }

    // ── bucket_id_for() ──

    #[test]
    fn bucket_id_for_minute_uses_minute_resolution() {
        let now = at(2026, 4, 27, 12, 3, 45);
        assert_eq!(bucket_id_for(now, &Scope::PerMinute), "202604271203");
    }

    #[test]
    fn bucket_id_for_hour_uses_hour_resolution() {
        let now = at(2026, 4, 27, 12, 3, 45);
        assert_eq!(bucket_id_for(now, &Scope::PerHour), "2026042712");
    }

    #[test]
    fn bucket_id_changes_at_minute_boundary() {
        let before = at(2026, 4, 27, 12, 3, 59);
        let after = at(2026, 4, 27, 12, 4, 0);
        assert_ne!(
            bucket_id_for(before, &Scope::PerMinute),
            bucket_id_for(after, &Scope::PerMinute)
        );
    }

    #[test]
    fn bucket_id_stable_within_minute() {
        let early = at(2026, 4, 27, 12, 3, 0);
        let late = at(2026, 4, 27, 12, 3, 59);
        assert_eq!(
            bucket_id_for(early, &Scope::PerMinute),
            bucket_id_for(late, &Scope::PerMinute)
        );
    }
}
