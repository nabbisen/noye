//! Audit log writes and queries.
//!
//! Every operator-initiated write to a Noye resource (and every system-initiated
//! retention sweep) is recorded in the `audit_logs` table for later forensic
//! review. Two insertion paths exist:
//!
//! - [`log`] — for human-actor writes (admin / member); takes a `Caller`.
//! - [`log_system`] — for cron / retention / migration writes; uses the
//!   sentinel actor `"system"`.
//!
//! ## Hash chain (since 0.27.2)
//!
//! Each row carries a `row_hash` (its own content hash) and a `prev_hash`
//! (the previous row's `row_hash`). Tampering with any row by `UPDATE` /
//! `DELETE` / out-of-order insertion breaks the chain at that row, and the
//! [`verify_chain`] function reports the break. See [`hash`] for the pure
//! computation helpers.
//!
//! ## Concurrency note
//!
//! The chain head is read with a `SELECT ... ORDER BY action_time DESC LIMIT 1`
//! immediately before each `INSERT`. Two concurrent writers can race and end up
//! with the same `prev_hash`, producing a fork in the chain. In normal Noye
//! operation that does not happen (cron is single-fiber, admin API is one
//! user), but it is acknowledged here so a future audit-log explosion (e.g.
//! Workers Queue fan-out) does not surprise anyone.

pub mod hash;
#[cfg(test)]
mod tests;

use noye_shared::{AuditEntry, Caller};
use wasm_bindgen::JsValue;
use worker::*;

use hash::{AuditRowFields, GENESIS_HASH, compute_row_hash};

/// The error `assert_hash_columns_present` returns for a database that
/// predates the hash-chain columns (tag 0.1.0, "Class A" in
/// requirements.md G-01) and has not yet been reconciled by migration
/// `0004` (see rfcs/handoffs/06-audit-actor-snapshot.md).
///
/// Without this check, such a database reports its migrations complete —
/// there is nothing left for it to apply, since migration `0002` that
/// once would have added the columns is retired — and then fails every
/// audit insert with a bare "no such column" error that the caller
/// currently discards (gap G-26). This turns that silent, potentially
/// months-long evidence gap into one legible, actionable error at the
/// first request.
pub const MISSING_HASH_COLUMNS_ERROR: &str = "audit_logs is missing prev_hash/row_hash — \
     this database predates 0.27.2 and has not been reconciled. Apply migration 0004.";

/// Pure classification of a D1 query error's text, split out from
/// [`assert_hash_columns_present`] so the decision logic is testable on
/// the host target without a D1 binding (NFR-QA-01).
///
/// D1 surfaces the underlying SQLite error text; a missing column reads
/// `"no such column: prev_hash"` or similar. Anything else (a transient
/// D1 error, the table not existing at all) is a different problem and
/// must not be misreported as this one.
fn classify_hash_column_query_error(raw: &str) -> String {
    if raw.contains("no such column") {
        MISSING_HASH_COLUMNS_ERROR.to_string()
    } else {
        format!("audit_logs schema check failed: {raw}")
    }
}

/// Probe row shape for [`assert_hash_columns_present`]. Both fields are
/// read but never used directly — a successful deserialization is itself
/// the proof the columns exist.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct HashColumnsProbe {
    prev_hash: Option<String>,
    row_hash: Option<String>,
}

/// Set once the schema probe in [`assert_hash_columns_present`] has
/// succeeded for this isolate. The condition it checks — whether
/// `audit_logs` carries the hash-chain columns — cannot change between
/// two requests served by the same isolate; changing it requires a
/// migration, which requires a deploy, which starts a fresh isolate.
/// Caching the success avoids paying a D1 round-trip on every request
/// (including ones that never touch `audit_logs`) for a condition that
/// is, in practice, static for the isolate's lifetime.
///
/// Deliberately caches only success. A failure is the rare path — it
/// means the deployment is misconfigured — so re-probing on every
/// request until the operator redeploys is cheap and keeps the
/// fail-closed behaviour unconditional rather than depending on cache
/// state. Found by independent review
/// (`.git-exclude/reviewed/013-audit-subjects-01-02.md` F-2); the
/// uncached version was mine.
static HASH_COLUMNS_CONFIRMED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Verify `audit_logs` carries `prev_hash` and `row_hash` before serving
/// any request. Intended to run once per request, alongside
/// `env_check::check_no_leaked_dev_fallbacks`, per
/// rfcs/handoffs/01-migration-applicability.md Build step 4 — but see
/// [`HASH_COLUMNS_CONFIRMED`] for why the underlying query itself only
/// runs once per isolate after the first success.
///
/// Deliberately queries the columns directly (`SELECT ... LIMIT 1`)
/// rather than introspecting via `PRAGMA table_info`: this is exactly
/// the query shape every audit insert depends on, so a pass here is a
/// direct guarantee rather than an inference from schema metadata.
pub async fn assert_hash_columns_present(db: &D1Database) -> Result<(), String> {
    if HASH_COLUMNS_CONFIRMED.get().is_some() {
        return Ok(());
    }

    match db
        .prepare("SELECT prev_hash, row_hash FROM audit_logs LIMIT 1")
        .first::<HashColumnsProbe>(None)
        .await
    {
        Ok(_) => {
            let _ = HASH_COLUMNS_CONFIRMED.set(());
            Ok(())
        }
        Err(e) => Err(classify_hash_column_query_error(&e.to_string())),
    }
}

/// Read the current chain head (most recent row's `row_hash`), or
/// [`GENESIS_HASH`] if the table is empty / contains only legacy
/// pre-hash-chain rows.
async fn current_head_hash(db: &D1Database) -> Result<String> {
    // We pick the most recent row that has a non-NULL row_hash. Legacy rows
    // (NULL row_hash) at the tail simply mean "no chain has been established
    // yet" — we treat that as genesis.
    let result = db
        .prepare(
            "SELECT row_hash FROM audit_logs
             WHERE row_hash IS NOT NULL
             ORDER BY action_time DESC
             LIMIT 1",
        )
        .first::<HeadRow>(None)
        .await?;
    Ok(result
        .and_then(|r| r.row_hash)
        .unwrap_or_else(|| GENESIS_HASH.to_string()))
}

#[derive(serde::Deserialize)]
struct HeadRow {
    row_hash: Option<String>,
}

pub async fn log(
    db: &D1Database,
    caller: &Caller,
    resource_type: &str,
    resource_id: &str,
    action_type: &str,
    previous_value: Option<&str>,
    new_value: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Compute hashes from the current chain head.
    let prev_hash = current_head_hash(db).await?;
    let row_hash = compute_row_hash(
        &prev_hash,
        &AuditRowFields {
            id: &id,
            action_time: &now,
            actor_id: &caller.user_id,
            actor_email: Some(&caller.email),
            resource_type,
            resource_id: Some(resource_id),
            action_type,
            previous_value,
            new_value,
            result: "success",
            ip_address: None,
        },
    );

    db.prepare(
        "INSERT INTO audit_logs
         (id, action_time, actor_id, actor_email, resource_type, resource_id,
          action_type, previous_value, new_value, result, prev_hash, row_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'success', ?10, ?11)",
    )
    .bind(&[
        id.into(),
        now.into(),
        caller.user_id.clone().into(),
        caller.email.clone().into(),
        resource_type.into(),
        resource_id.into(),
        action_type.into(),
        previous_value.map(JsValue::from).unwrap_or(JsValue::NULL),
        new_value.map(JsValue::from).unwrap_or(JsValue::NULL),
        prev_hash.into(),
        row_hash.into(),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn log_system(
    db: &D1Database,
    resource_type: &str,
    resource_id: &str,
    action_type: &str,
    details: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let prev_hash = current_head_hash(db).await?;
    let row_hash = compute_row_hash(
        &prev_hash,
        &AuditRowFields {
            id: &id,
            action_time: &now,
            actor_id: "system",
            actor_email: Some("system"),
            resource_type,
            resource_id: Some(resource_id),
            action_type,
            previous_value: None,
            new_value: details,
            result: "success",
            ip_address: None,
        },
    );

    db.prepare(
        "INSERT INTO audit_logs
         (id, action_time, actor_id, actor_email, resource_type, resource_id,
          action_type, new_value, result, prev_hash, row_hash)
         VALUES (?1, ?2, 'system', 'system', ?3, ?4, ?5, ?6, 'success', ?7, ?8)",
    )
    .bind(&[
        id.into(),
        now.into(),
        resource_type.into(),
        resource_id.into(),
        action_type.into(),
        details.map(JsValue::from).unwrap_or(JsValue::NULL),
        prev_hash.into(),
        row_hash.into(),
    ])?
    .run()
    .await?;
    Ok(())
}

/// Record a successful login. The event is attributed to the user (not
/// `system`) so it shows up in their `/me/security` page; the chain
/// inclusion mirrors `log` so the row is part of the same tamper-detect
/// chain as everything else.
///
/// Distinct from `log` because it does not require a `Caller` — the
/// caller doesn't exist yet (the session that would back it was just
/// created seconds ago and the browser hasn't yet sent its cookie back).
pub async fn log_login(
    db: &D1Database,
    user_id: &str,
    user_email: &str,
    ip_address: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let prev_hash = current_head_hash(db).await?;
    let row_hash = compute_row_hash(
        &prev_hash,
        &AuditRowFields {
            id: &id,
            action_time: &now,
            actor_id: user_id,
            actor_email: Some(user_email),
            resource_type: "session",
            resource_id: Some(user_id),
            action_type: "login",
            previous_value: None,
            new_value: None,
            result: "success",
            ip_address,
        },
    );

    db.prepare(
        "INSERT INTO audit_logs
         (id, action_time, actor_id, actor_email, resource_type, resource_id,
          action_type, result, ip_address, prev_hash, row_hash)
         VALUES (?1, ?2, ?3, ?4, 'session', ?5, 'login', 'success', ?6, ?7, ?8)",
    )
    .bind(&[
        id.into(),
        now.into(),
        user_id.into(),
        user_email.into(),
        user_id.into(),
        ip_address.map(JsValue::from).unwrap_or(JsValue::NULL),
        prev_hash.into(),
        row_hash.into(),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn list_recent(db: &D1Database, limit: i64) -> Result<Vec<AuditEntry>> {
    let results = db
        .prepare("SELECT * FROM audit_logs ORDER BY action_time DESC LIMIT ?1")
        .bind(&[JsValue::from(limit)])?
        .all()
        .await?;
    results.results::<AuditEntry>()
}

/// List the most recent login-flow events for a specific actor.
///
/// Used by the `/me/security` UI to show "your recent sign-ins". Filters
/// on `actor_id = ?` AND `action_type = 'login'` so a user only sees their
/// own history. Other users' login activity is admin-only via `list_recent`.
pub async fn list_login_history(
    db: &D1Database,
    actor_id: &str,
    limit: i64,
) -> Result<Vec<AuditEntry>> {
    let results = db
        .prepare(
            "SELECT * FROM audit_logs
             WHERE actor_id = ?1 AND action_type = 'login'
             ORDER BY action_time DESC
             LIMIT ?2",
        )
        .bind(&[actor_id.into(), JsValue::from(limit)])?
        .all()
        .await?;
    results.results::<AuditEntry>()
}

// ─────────────────────────────────────────────
//  Chain verification
// ─────────────────────────────────────────────

/// Per-row fields the verifier reads from D1. Mirrors `audit_logs` plus the
/// new chain columns.
#[derive(Debug, Clone, serde::Deserialize)]
struct ChainRow {
    id: String,
    action_time: String,
    actor_id: String,
    actor_email: Option<String>,
    resource_type: String,
    resource_id: Option<String>,
    action_type: String,
    previous_value: Option<String>,
    new_value: Option<String>,
    result: String,
    ip_address: Option<String>,
    prev_hash: Option<String>,
    row_hash: Option<String>,
}

/// Outcome of a chain verification pass over the entire `audit_logs` table.
///
/// `legacy_rows` are rows written before 0.27.2 (NULL hash columns); they
/// are tallied separately because their absence of a hash is expected, not a
/// tampering indicator.
#[derive(Debug, serde::Serialize)]
pub struct ChainVerification {
    pub total_rows: usize,
    pub legacy_rows: usize,
    pub verified_rows: usize,
    pub tampered_rows: Vec<TamperedRow>,
}

/// One row that failed the chain check, with a human-readable reason.
#[derive(Debug, serde::Serialize)]
pub struct TamperedRow {
    pub id: String,
    pub action_time: String,
    pub reason: &'static str,
}

/// Walk the entire `audit_logs` table in `action_time ASC` order and verify
/// the hash chain. Returns a structured report.
///
/// Cost: one full-table scan. For small Noye deployments (a few thousand
/// rows even after a year) this is fine; if the table grows larger, consider
/// chunking the verification by date range.
pub async fn verify_chain(db: &D1Database) -> Result<ChainVerification> {
    let rows = db
        .prepare("SELECT * FROM audit_logs ORDER BY action_time ASC, id ASC")
        .all()
        .await?
        .results::<ChainRow>()?;

    let mut total_rows = 0usize;
    let mut legacy_rows = 0usize;
    let mut verified_rows = 0usize;
    let mut tampered_rows = Vec::new();

    let mut expected_prev = GENESIS_HASH.to_string();

    for row in &rows {
        total_rows += 1;

        match (row.prev_hash.as_deref(), row.row_hash.as_deref()) {
            (None, None) => {
                // Legacy row predating the hash chain. Skip but count.
                legacy_rows += 1;
                continue;
            }
            (Some(stored_prev), Some(stored_row_hash)) => {
                // Check linkage: stored prev_hash must equal the chain's
                // running expected_prev (i.e. the previous verified row's
                // row_hash, or GENESIS_HASH for the first non-legacy row).
                if stored_prev != expected_prev {
                    tampered_rows.push(TamperedRow {
                        id: row.id.clone(),
                        action_time: row.action_time.clone(),
                        reason: "prev_hash does not match the prior row's row_hash",
                    });
                    // Don't update expected_prev — leaving it pinned means
                    // every subsequent row also fails fast, which is the
                    // correct way to surface a deletion mid-chain.
                    continue;
                }

                // Recompute row_hash from the row's own content + stored prev.
                let recomputed = compute_row_hash(
                    stored_prev,
                    &AuditRowFields {
                        id: &row.id,
                        action_time: &row.action_time,
                        actor_id: &row.actor_id,
                        actor_email: row.actor_email.as_deref(),
                        resource_type: &row.resource_type,
                        resource_id: row.resource_id.as_deref(),
                        action_type: &row.action_type,
                        previous_value: row.previous_value.as_deref(),
                        new_value: row.new_value.as_deref(),
                        result: &row.result,
                        ip_address: row.ip_address.as_deref(),
                    },
                );
                if recomputed != stored_row_hash {
                    tampered_rows.push(TamperedRow {
                        id: row.id.clone(),
                        action_time: row.action_time.clone(),
                        reason: "row_hash does not match recomputed value (row contents tampered)",
                    });
                    continue;
                }

                verified_rows += 1;
                expected_prev = stored_row_hash.to_string();
            }
            _ => {
                // Half-set hash columns shouldn't occur. Treat as tampering.
                tampered_rows.push(TamperedRow {
                    id: row.id.clone(),
                    action_time: row.action_time.clone(),
                    reason: "exactly one of prev_hash / row_hash is NULL — corrupt row",
                });
            }
        }
    }

    Ok(ChainVerification {
        total_rows,
        legacy_rows,
        verified_rows,
        tampered_rows,
    })
}
