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
//! ## Order comes from the links, never from a sort (subject 05, DEC-020)
//!
//! The chain's order **is insertion order**. It is carried entirely by
//! `prev_hash → row_hash` links — nothing else records it. Before subject
//! 05, [`verify_chain`] recovered order by `ORDER BY action_time ASC, id
//! ASC`, which is not sound: `action_time` has one-second resolution and
//! `id` is a random UUID, so no sort over `(action_time, id)` is monotonic
//! with insertion. A row written into a second that already has rows in
//! it landed at a random position in that sort, and whenever it sorted
//! before the row it actually chained to, the verifier reported it — and
//! every row after it — as tampered. Twenty rows in one second failed
//! verification virtually 100% of the time; a two-row configuration
//! import failed roughly half the time. See
//! `rfcs/handoffs/05-audit-chain-ordering.md` and
//! `.git-exclude/reviewed/025-subject-05-defective-fix.md` for the full
//! analysis, including why *matching* the sort's tie-breaks (rather than
//! abandoning sorting) does not fix this: it changes which row is chosen
//! as head, not whether a newly inserted row sorts after it.
//!
//! [`verify_chain`] (via [`walk_chain`]) now follows the links directly:
//! it indexes rows by `prev_hash` and walks forward from [`hash::GENESIS_HASH`].
//! **Do not "optimise" any query here back into an `ORDER BY` that
//! assumes sort order reflects chain order.** A row's position in
//! whatever order a `SELECT` happens to return is not, and has never
//! been, informative — only the links are. The `ORDER BY action_time
//! ASC` on the fetch in [`verify_chain`] exists only so the query is
//! deterministic across runs; correctness never depends on it (see
//! `walk_chain`'s own tests, particularly T-21).
//!
//! ## Concurrency note
//!
//! The chain head is found by `walk_chain`'s own forward walk — see
//! `current_head_hash` (unchanged by subject 05; see its own doc comment
//! for the corresponding tail-query considerations). Two concurrent
//! writers can still race and end up computing the same `prev_hash`,
//! producing a genuine fork in the chain — this is what
//! [`OrphanedRow`]/`tampered_rows` distinguish a *deletion's* orphaned
//! successors from. In normal Noye operation a concurrent race does not
//! happen (cron is single-fiber, admin API is one user), but it is
//! acknowledged here so a future audit-log explosion (e.g. Workers Queue
//! fan-out) does not surprise anyone.

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
///
/// **Deliberately unchanged by subject 05, pending an escalated design
/// question (T-23b, `rfcs/handoffs/05-audit-chain-ordering.md` Build
/// step 3).** The handoff's specified replacement — the row whose
/// `row_hash` is no other row's `prev_hash` — was reproduced against a
/// live SQLite database and confirmed to return more than one row after
/// an ordinary, single-writer mid-chain deletion (T-22's own scenario),
/// not only under a genuine concurrent-writer fork: the deleted row's
/// direct predecessor and the chain's true tail both satisfy "nothing
/// currently in the table points to me," because the query cannot
/// distinguish genesis-reachable rows from an orphaned island's own
/// local tail. See the escalation in
/// `.git-exclude/review-request/` for Subject 05 for the reproduction
/// and a proposed alternative (deriving the head from the same
/// genesis-walk `walk_chain` performs). Per the handoff's own Escalate
/// table, this is reported rather than redesigned unilaterally. Still
/// has the pre-existing action_time-ordering issue this subject
/// otherwise closes elsewhere — see the module docs above.
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
/// Every row lands in exactly one of four classes (subject 05, DEC-020;
/// `external-design.md` §5, S-11):
///
/// - `legacy_rows` — written before 0.27.2 (NULL hash columns); expected,
///   not a tampering indicator.
/// - `verified_rows` — reached from genesis, content matches its stored
///   `row_hash`.
/// - `tampered_rows` — reached, but content does not match.
/// - `orphaned_rows` — carries hashes but was never reached from genesis.
///
/// `orphaned` is deliberately its own class, not folded into `tampered`:
/// a deleted row makes its successors unreachable, and reporting them as
/// tampered would name the wrong rows as damaged (FR-AUD-05).
///
/// A `prev_hash → row_hash` cycle (a row whose stored `row_hash` loops
/// the walk back to an already-visited row — trivial for anything with
/// `INSERT` access to construct, since nothing enforces `row_hash` is
/// the output of an actual hash) reports as `tampered`, with a reason
/// naming the cycle specifically — not a fifth class, since [`walk_chain`]
/// must still terminate and produce a report, never hang, on any input.
#[derive(Debug, serde::Serialize)]
pub struct ChainVerification {
    pub total_rows: usize,
    pub legacy_rows: usize,
    pub verified_rows: usize,
    pub tampered_rows: Vec<TamperedRow>,
    pub orphaned_rows: Vec<OrphanedRow>,
}

/// One row that failed the chain check, with a human-readable reason.
#[derive(Debug, serde::Serialize)]
pub struct TamperedRow {
    pub id: String,
    pub action_time: String,
    pub reason: &'static str,
}

/// One row carrying hash-chain columns that was not reached by following
/// the chain from genesis — typically because the row it should chain
/// from was deleted, or because it lost a fork (see [`walk_chain`]).
#[derive(Debug, serde::Serialize)]
pub struct OrphanedRow {
    pub id: String,
    pub action_time: String,
}

/// Walk the entire `audit_logs` table and verify the hash chain by
/// following `prev_hash → row_hash` links from [`GENESIS_HASH`]. Returns
/// a structured report.
///
/// Cost: one full-table scan. For small Noye deployments (a few thousand
/// rows even after a year) this is fine; if the table grows larger, consider
/// chunking the verification by date range.
///
/// The `SELECT` carries no `ORDER BY` load-bearing for correctness — see
/// [`walk_chain`] and the module docs above for why the chain's order is
/// never recovered by sorting.
pub async fn verify_chain(db: &D1Database) -> Result<ChainVerification> {
    let rows = db
        .prepare("SELECT * FROM audit_logs ORDER BY action_time ASC")
        .all()
        .await?
        .results::<ChainRow>()?;

    Ok(walk_chain(&rows))
}

/// Classify every row in `rows` by following the chain's own
/// `prev_hash → row_hash` links from [`GENESIS_HASH`], rather than by
/// sorting rows into an order and assuming adjacency-in-sort implies
/// chain adjacency (subject 05, DEC-020 — see the module docs above for
/// why sorting cannot recover insertion order).
///
/// Pure and host-testable (NFR-QA-01); [`verify_chain`] is a thin
/// fetch-then-walk wrapper — the query already loads the whole table,
/// so this adds no I/O.
///
/// **The result does not depend on the order `rows` arrives in (T-21).**
/// Rows are indexed by `prev_hash`, and the walk starts at genesis and
/// follows the unique link forward; the input order never determines
/// which row comes "next." The one place multiple rows can compete —
/// two rows sharing a `prev_hash`, i.e. a fork — is broken by sorting
/// only those candidates on `(action_time, id)`, which is a bounded,
/// deterministic tiebreak among an anomalous handful of rows, not a
/// mechanism this function relies on to reconstruct the whole chain.
///
/// **Must terminate, and produce a report rather than hang, on any
/// input** — `rows` is not only what [`log`]/[`log_system`]/[`log_login`]
/// write; it is whatever is in `audit_logs`, which anyone with `INSERT`
/// access to the table can shape arbitrarily. The walk therefore refuses
/// to revisit an already-`reached` row: without that check, a row whose
/// stored `row_hash` loops back to an earlier hash in the walk (nothing
/// requires `row_hash` to be an actual hash of anything) would advance
/// `expected` back to itself forever. A tamper-evidence check an
/// attacker can hang by writing one row is not a tamper-evidence check.
fn walk_chain(rows: &[ChainRow]) -> ChainVerification {
    let mut total_rows = 0usize;
    let mut legacy_rows = 0usize;

    // Hashed rows (both columns present), indexed by prev_hash so the
    // walk can find "whatever chains from this hash" in O(1) instead of
    // scanning. Multiple entries under one key means a fork (T-23a).
    let mut by_prev_hash: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::new();

    for (idx, row) in rows.iter().enumerate() {
        total_rows += 1;
        match (row.prev_hash.as_deref(), row.row_hash.as_deref()) {
            (None, None) => legacy_rows += 1,
            (Some(prev), Some(_)) => by_prev_hash.entry(prev).or_default().push(idx),
            _ => {} // half-set columns: classified in the final pass, below
        }
    }

    // Deterministic, input-order-independent tiebreak within a fork.
    for candidates in by_prev_hash.values_mut() {
        candidates.sort_by(|&a, &b| {
            (&rows[a].action_time, &rows[a].id).cmp(&(&rows[b].action_time, &rows[b].id))
        });
    }

    let mut verified_rows = 0usize;
    let mut tampered_rows = Vec::new();
    let mut reached = vec![false; rows.len()];

    let mut expected = GENESIS_HASH.to_string();
    while let Some(candidates) = by_prev_hash.get(expected.as_str()) {
        let idx = candidates[0]; // fork tiebreak already applied above

        // `rows` is attacker-influenced input (anything `INSERT`-able
        // into audit_logs, not only what `log()` writes), so the walk
        // must be total: a row_hash can equal an earlier row's own
        // row_hash — nothing stops the same TEXT value being reused —
        // which loops `expected` back to an already-`reached` index
        // forever. `row_hash` being a hash of content that itself
        // includes `prev_hash` makes a *valid* cycle a preimage attack
        // (infeasible); a row written directly with an arbitrary
        // row_hash value needs no such attack, so this check cannot be
        // skipped as "can't happen". Reported distinctly from a content
        // mismatch — "tampered" alone doesn't tell an operator the
        // structure loops.
        if reached[idx] {
            tampered_rows.push(TamperedRow {
                id: rows[idx].id.clone(),
                action_time: rows[idx].action_time.clone(),
                reason: "prev_hash → row_hash chain loops back on an already-visited row",
            });
            break;
        }
        let row = &rows[idx];
        reached[idx] = true;

        // Indexed only when both are Some, so both unwraps are safe.
        let stored_prev = row.prev_hash.as_deref().unwrap();
        let stored_row_hash = row.row_hash.as_deref().unwrap();

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

        if recomputed == stored_row_hash {
            verified_rows += 1;
        } else {
            tampered_rows.push(TamperedRow {
                id: row.id.clone(),
                action_time: row.action_time.clone(),
                reason: "row_hash does not match recomputed value (row contents tampered)",
            });
        }

        // Advance using the row's own *stored* row_hash regardless of
        // whether its content matched — forward-linking depends only on
        // the stored value, so a tampered row's successors can still be
        // found and correctly verified. This is what makes "only that
        // row" (T-23) hold: tampering one row does not orphan the rows
        // chained after it.
        expected = stored_row_hash.to_string();
    }

    // Final pass: anything not legacy and not reached during the walk —
    // including a fork's losing branch and everything chained onto it —
    // carries hashes but was never reached from genesis.
    let mut orphaned_rows = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        if reached[idx] {
            continue;
        }
        match (row.prev_hash.as_deref(), row.row_hash.as_deref()) {
            (None, None) => {} // legacy, already counted
            (Some(_), Some(_)) => orphaned_rows.push(OrphanedRow {
                id: row.id.clone(),
                action_time: row.action_time.clone(),
            }),
            _ => tampered_rows.push(TamperedRow {
                id: row.id.clone(),
                action_time: row.action_time.clone(),
                reason: "exactly one of prev_hash / row_hash is NULL — corrupt row",
            }),
        }
    }

    // Stable output ordering for display — independent of the walk order
    // and of any fork tiebreak, so the report reads the same regardless
    // of input order (T-21 applies to the report's content, not just its
    // counts).
    tampered_rows.sort_by(|a, b| a.action_time.cmp(&b.action_time));
    orphaned_rows.sort_by(|a, b| a.action_time.cmp(&b.action_time));

    ChainVerification {
        total_rows,
        legacy_rows,
        verified_rows,
        tampered_rows,
        orphaned_rows,
    }
}
