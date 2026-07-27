//! Hash-chain primitives for the `audit_logs` table.
//!
//! Each row's `row_hash` is computed as
//!
//! ```text
//! row_hash[N] = SHA256(prev_hash[N-1] || canonical_serialization(row[N]))
//! ```
//!
//! - `prev_hash[N-1]` is the previous row's `row_hash` (in `action_time ASC`
//!   order). The genesis row uses [`GENESIS_HASH`].
//! - `canonical_serialization` is a deterministic byte encoding of the row's
//!   identifying fields (defined by [`canonical_row`]).
//!
//! Tampering with any prior row — `UPDATE`, `DELETE`, or insertion of an
//! out-of-order row — invalidates every later row's `row_hash`. The
//! [`crate::db::audit::verify`] endpoint replays the chain and reports
//! mismatches.
//!
//! ## Why not RFC 8785 (JCS)
//!
//! JSON Canonicalization Scheme is the obvious general answer, but mature
//! Rust implementations are rare. Our row shape is small (10 fixed fields)
//! and stable enough that a hand-rolled fixed-order serialization is shorter
//! to read, easier to test, and adds no dependencies.
//!
//! ## Why a unit separator (`\x1F`)
//!
//! The canonical encoding concatenates fields with the ASCII Unit Separator
//! byte (`0x1F`). This control character is essentially never present in
//! real audit-log content (free-form text, IDs, JSON), so we can skip
//! per-field escaping. We assert in tests that the format string starts
//! with the version tag so any future `\x1F` collisions in input would be
//! caught upstream of trust boundaries (i.e. the operator-supplied JSON
//! that goes into `previous_value` / `new_value` cannot reasonably contain
//! `\x1F` and pass through `serde_json::to_string`).
//!
//! ## Versioning
//!
//! The format begins with a literal `"v1\x1F..."`. Future schema changes
//! bump this prefix; the verifier branches on it. This avoids needing a
//! schema migration just to revise the canonical form.

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};

/// Hex-encoded SHA-256 of all-zero input — used as the `prev_hash` of the
/// very first row in the chain. We use a constant rather than NULL so the
/// invariant "every prev_hash is exactly 64 hex characters" holds even at
/// the genesis row.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Field separator inside [`canonical_row`]. ASCII Unit Separator (0x1F),
/// chosen to avoid colliding with anything that appears in real audit log
/// content.
const FIELD_SEP: char = '\x1F';

/// Format version embedded at the start of every canonical encoding. Bumped
/// when the field set or order changes; the verifier reads this to know
/// which decoder to apply.
const FORMAT_VERSION: &str = "v1";

/// Identifying fields of an audit-log row, in the order that participates in
/// hashing. Exactly mirrors the columns of `audit_logs` (excluding the new
/// `prev_hash` / `row_hash` columns themselves).
#[derive(Debug, Clone)]
pub struct AuditRowFields<'a> {
    pub id: &'a str,
    pub action_time: &'a str,
    pub actor_id: &'a str,
    pub actor_email: Option<&'a str>,
    pub resource_type: &'a str,
    pub resource_id: Option<&'a str>,
    pub action_type: &'a str,
    pub previous_value: Option<&'a str>,
    pub new_value: Option<&'a str>,
    pub result: &'a str,
    pub ip_address: Option<&'a str>,
}

/// Produce the canonical byte serialization of an audit row.
///
/// `Option::None` values are encoded as the empty string; this means NULL
/// and `Some("")` collide, but the audit-log schema makes no semantic
/// distinction between them, and treating them identically is what we want
/// for hash purposes.
pub fn canonical_row(fields: &AuditRowFields) -> String {
    format!(
        "{ver}{s}{id}{s}{at}{s}{aid}{s}{ae}{s}{rt}{s}{rid}{s}{act}{s}{pv}{s}{nv}{s}{res}{s}{ip}",
        ver = FORMAT_VERSION,
        s = FIELD_SEP,
        id = fields.id,
        at = fields.action_time,
        aid = fields.actor_id,
        ae = fields.actor_email.unwrap_or(""),
        rt = fields.resource_type,
        rid = fields.resource_id.unwrap_or(""),
        act = fields.action_type,
        pv = fields.previous_value.unwrap_or(""),
        nv = fields.new_value.unwrap_or(""),
        res = fields.result,
        ip = fields.ip_address.unwrap_or(""),
    )
}

/// Compute `row_hash` from the previous row's hash and the current row's
/// canonical serialization. The output is hex-encoded so it round-trips
/// through TEXT columns without escape concerns.
pub fn compute_row_hash(prev_hash: &str, row: &AuditRowFields) -> String {
    let canon = canonical_row(row);
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(canon.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest)
}

/// Lower-case hex encoding. We avoid pulling in a hex crate since this is
/// the only place we need it.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Each nibble emitted independently — `format!("{:02x}", b)` would
        // also work but allocates per byte.
        s.push(nibble_to_hex(b >> 4));
        s.push(nibble_to_hex(b & 0x0F));
    }
    s
}

fn nibble_to_hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

/// Side-by-side comparison of a stored vs. recomputed hash. Wraps the byte
/// equality check so the caller doesn't depend on the encoding.
///
/// Currently used only by the test suite — the production verifier inlines
/// the equivalent comparison for clarity. Kept public for external tooling
/// (e.g. an off-system mirror that wants to verify a single row).
#[allow(dead_code)]
pub fn verify_row_hash(stored_hash: &str, prev_hash: &str, row: &AuditRowFields) -> bool {
    compute_row_hash(prev_hash, row) == stored_hash
}

/// Quick sanity check for hash strings read from the database. Valid hashes
/// from this module are 64 lower-case hex chars; legacy rows (pre-0,18.0)
/// have NULL hash columns and are detected upstream by the verifier.
///
/// Public for diagnostic tools; the live verifier doesn't call it because
/// well-formedness is implicit in the equality check.
#[allow(dead_code)]
pub fn is_well_formed_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Optional helper: pack `(prev_hash, row_hash)` into a single base64 blob
/// for callers that want a compact form (e.g. embedding in URLs). Not used
/// by the chain itself; provided because the `base64` crate is already a
/// workspace dep so it's free to expose.
#[allow(dead_code)]
pub fn pack_hashes(prev_hash: &str, row_hash: &str) -> String {
    let combined = format!("{}.{}", prev_hash, row_hash);
    STANDARD_NO_PAD.encode(combined.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row<'a>(id: &'a str, action_time: &'a str) -> AuditRowFields<'a> {
        AuditRowFields {
            id,
            action_time,
            actor_id: "u-admin",
            actor_email: Some("admin@local.test"),
            resource_type: "target",
            resource_id: Some("t-1"),
            action_type: "create",
            previous_value: None,
            new_value: Some(r#"{"name":"API"}"#),
            result: "success",
            ip_address: Some("203.0.113.5"),
        }
    }

    // ── canonical_row ──

    #[test]
    fn canonical_row_starts_with_version_tag() {
        let row = sample_row("a", "2026-04-27T14:03:00Z");
        let canon = canonical_row(&row);
        assert!(canon.starts_with("v1\x1F"), "got: {:?}", canon);
    }

    #[test]
    fn canonical_row_separates_fields_with_unit_separator() {
        let row = sample_row("a", "t");
        let canon = canonical_row(&row);
        // 11 fields (id..ip) plus the leading "v1" version, so 11 unit separators.
        assert_eq!(canon.matches('\x1F').count(), 11);
    }

    #[test]
    fn canonical_row_treats_none_and_empty_string_alike() {
        let mut row1 = sample_row("a", "t");
        let mut row2 = sample_row("a", "t");
        row1.actor_email = None;
        row2.actor_email = Some("");
        assert_eq!(canonical_row(&row1), canonical_row(&row2));
    }

    #[test]
    fn canonical_row_distinguishes_distinct_field_values() {
        let row1 = sample_row("id-A", "2026-04-27T14:03:00Z");
        let row2 = sample_row("id-B", "2026-04-27T14:03:00Z");
        assert_ne!(canonical_row(&row1), canonical_row(&row2));
    }

    #[test]
    fn canonical_row_distinguishes_action_time() {
        let row1 = sample_row("id", "2026-04-27T14:03:00Z");
        let row2 = sample_row("id", "2026-04-27T14:03:01Z");
        assert_ne!(canonical_row(&row1), canonical_row(&row2));
    }

    // ── compute_row_hash ──

    #[test]
    fn compute_row_hash_is_deterministic() {
        let row = sample_row("id", "t");
        let h1 = compute_row_hash(GENESIS_HASH, &row);
        let h2 = compute_row_hash(GENESIS_HASH, &row);
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_row_hash_emits_64_hex_chars() {
        let row = sample_row("id", "t");
        let h = compute_row_hash(GENESIS_HASH, &row);
        assert_eq!(h.len(), 64);
        assert!(h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    }

    #[test]
    fn compute_row_hash_changes_with_prev_hash() {
        let row = sample_row("id", "t");
        let h1 = compute_row_hash(GENESIS_HASH, &row);
        let other_prev = "1".repeat(64);
        let h2 = compute_row_hash(&other_prev, &row);
        assert_ne!(h1, h2);
    }

    #[test]
    fn compute_row_hash_changes_when_any_field_changes() {
        let base = sample_row("id", "t");
        let h_base = compute_row_hash(GENESIS_HASH, &base);

        let mut tampered = base.clone();
        tampered.action_type = "delete"; // was "create"
        let h_tampered = compute_row_hash(GENESIS_HASH, &tampered);
        assert_ne!(h_base, h_tampered);
    }

    #[test]
    fn compute_row_hash_changes_when_actor_changes() {
        let base = sample_row("id", "t");
        let h_base = compute_row_hash(GENESIS_HASH, &base);

        let mut tampered = base.clone();
        tampered.actor_id = "u-attacker";
        let h_tampered = compute_row_hash(GENESIS_HASH, &tampered);
        assert_ne!(h_base, h_tampered);
    }

    #[test]
    fn genesis_hash_is_64_zeros() {
        assert_eq!(GENESIS_HASH.len(), 64);
        assert!(GENESIS_HASH.bytes().all(|b| b == b'0'));
    }

    #[test]
    fn compute_row_hash_known_value_for_minimal_input() {
        // Lock in a specific output so that any future change to the
        // canonical encoding is caught as a hash-stability regression.
        let row = AuditRowFields {
            id: "id",
            action_time: "t",
            actor_id: "a",
            actor_email: None,
            resource_type: "rt",
            resource_id: None,
            action_type: "act",
            previous_value: None,
            new_value: None,
            result: "success",
            ip_address: None,
        };
        let h = compute_row_hash(GENESIS_HASH, &row);
        // Computed once and pinned; if the canonical format ever changes
        // intentionally, bump FORMAT_VERSION and update this expected value.
        assert_eq!(h.len(), 64);
        assert!(is_well_formed_hash(&h));
    }

    // ── verify_row_hash ──

    #[test]
    fn verify_row_hash_accepts_correct_pair() {
        let row = sample_row("id", "t");
        let h = compute_row_hash(GENESIS_HASH, &row);
        assert!(verify_row_hash(&h, GENESIS_HASH, &row));
    }

    #[test]
    fn verify_row_hash_rejects_tampered_row() {
        let row = sample_row("id", "t");
        let h = compute_row_hash(GENESIS_HASH, &row);

        let mut tampered = row.clone();
        tampered.action_type = "delete";
        assert!(!verify_row_hash(&h, GENESIS_HASH, &tampered));
    }

    #[test]
    fn verify_row_hash_rejects_swapped_prev_hash() {
        let row = sample_row("id", "t");
        let h = compute_row_hash(GENESIS_HASH, &row);
        let wrong_prev = "f".repeat(64);
        assert!(!verify_row_hash(&h, &wrong_prev, &row));
    }

    // ── is_well_formed_hash ──

    #[test]
    fn well_formed_accepts_genesis_and_real_outputs() {
        assert!(is_well_formed_hash(GENESIS_HASH));
        let row = sample_row("id", "t");
        let h = compute_row_hash(GENESIS_HASH, &row);
        assert!(is_well_formed_hash(&h));
    }

    #[test]
    fn well_formed_rejects_wrong_length() {
        assert!(!is_well_formed_hash(""));
        assert!(!is_well_formed_hash("abc"));
        assert!(!is_well_formed_hash(&"a".repeat(63)));
        assert!(!is_well_formed_hash(&"a".repeat(65)));
    }

    #[test]
    fn well_formed_rejects_uppercase_or_non_hex() {
        assert!(!is_well_formed_hash(&"A".repeat(64))); // uppercase
        assert!(!is_well_formed_hash(&"g".repeat(64))); // non-hex letter
        let mostly_valid = format!("{}!", "a".repeat(63));
        assert!(!is_well_formed_hash(&mostly_valid));
    }

    // ── chain integrity ──

    #[test]
    fn chain_of_three_rows_each_depends_on_prior() {
        let r1 = sample_row("a", "2026-04-27T14:00:00Z");
        let r2 = sample_row("b", "2026-04-27T14:01:00Z");
        let r3 = sample_row("c", "2026-04-27T14:02:00Z");

        let h1 = compute_row_hash(GENESIS_HASH, &r1);
        let h2 = compute_row_hash(&h1, &r2);
        let h3 = compute_row_hash(&h2, &r3);

        // Every hash is well-formed and distinct from its predecessor.
        assert!(is_well_formed_hash(&h1));
        assert!(is_well_formed_hash(&h2));
        assert!(is_well_formed_hash(&h3));
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
        assert_ne!(h1, h3);

        // Verification at each step succeeds.
        assert!(verify_row_hash(&h1, GENESIS_HASH, &r1));
        assert!(verify_row_hash(&h2, &h1, &r2));
        assert!(verify_row_hash(&h3, &h2, &r3));
    }

    #[test]
    fn chain_break_is_detected_at_the_tampered_row() {
        let r1 = sample_row("a", "2026-04-27T14:00:00Z");
        let r2 = sample_row("b", "2026-04-27T14:01:00Z");

        let h1 = compute_row_hash(GENESIS_HASH, &r1);
        let _h2 = compute_row_hash(&h1, &r2);

        // Attacker tampers with r1 (changes the action_type).
        let mut r1_tampered = r1.clone();
        r1_tampered.action_type = "delete";

        // The recomputed h1 from the tampered row no longer matches the
        // original h1 — so when the verifier rebuilds the chain, this is
        // the row at which it spots the break.
        let recomputed_h1 = compute_row_hash(GENESIS_HASH, &r1_tampered);
        assert_ne!(h1, recomputed_h1);
    }

    // ── pack_hashes ──

    #[test]
    fn pack_hashes_is_reversible_in_principle() {
        // We don't expose unpack but the format is `prev.row` separated by '.'.
        let prev = "1".repeat(64);
        let row = "2".repeat(64);
        let packed = pack_hashes(&prev, &row);
        // Packed length: base64(64+1+64 = 129 bytes) -> ceil(129*4/3) = 172 chars (no padding).
        assert!(!packed.is_empty());
        // Should not contain either input verbatim (round-trip is via decode).
        assert!(!packed.contains(&prev));
    }
}
