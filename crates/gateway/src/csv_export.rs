//! CSV encoding for SLA reports.
//!
//! Produces RFC 4180 output with a UTF-8 BOM prefix so Microsoft Excel reads
//! the file as UTF-8 rather than Shift_JIS / Windows-1252 / Latin-1. Without
//! the BOM, non-ASCII target names land in Excel as mojibake.
//!
//! ## Why a hand-rolled encoder
//!
//! Adding the `csv` crate would be straightforward but pulls in transitive
//! dependencies that increase the WASM bundle size for two functions of
//! ~40 lines. The encoder here is small enough to test exhaustively.
//!
//! ## What this module is NOT
//!
//! Generic. There is no reason for someone to add `encode_users(...)` here.
//! The two encoders below exist because they have stable, well-defined
//! shapes that are documented in `docs/src/api.md`. Anything else — a CSV view
//! of `audit_logs`, of `users`, of raw `check_results` — should live in its
//! own module so each CSV's column contract stays visible at the function
//! it represents.

use noye_shared::{Incident, SlaSummary};

/// UTF-8 byte-order-mark. Required by Excel for non-ASCII field values.
pub const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Encode a single field per RFC 4180.
///
/// - Empty → empty
/// - Contains any of `,` `"` CR LF → wrap in `"`, double internal `"`
/// - Otherwise → return as-is
fn quote_field(s: &str) -> String {
    let needs_quoting = s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r');
    if needs_quoting {
        let escaped = s.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        s.to_string()
    }
}

/// Format a uptime ratio for CSV. Plain decimal, no `%`, four decimal places
/// — enough to distinguish "five nines" (99.999%) from "four nines"
/// (99.99%) and still sortable in any spreadsheet that recognises decimals.
fn format_ratio(r: f64) -> String {
    format!("{:.4}", r * 100.0)
}

/// Subject 13 (FR-SLA-09): empty cell, not a claimed `100.0000`, when the
/// entire window was excluded -- same empty-not-em-dash convention this
/// file already uses for `mttr_seconds` (CSV, unlike the HTML UI, has no
/// em dash; an empty cell is a spreadsheet's native "no value").
fn format_ratio_opt(r: Option<f64>) -> String {
    match r {
        Some(v) => format_ratio(v),
        None => String::new(),
    }
}

/// Format a duration in seconds as a plain integer. Spreadsheets are better
/// at math than at parsing "1d 4h 30m"; we leave the human formatting to the
/// reader.
fn format_seconds(s: i64) -> String {
    s.to_string()
}

/// Join a row of fields into a CSV line ending with CRLF.
fn write_row(fields: &[&str]) -> String {
    let escaped: Vec<String> = fields.iter().map(|f| quote_field(f)).collect();
    let mut out = escaped.join(",");
    out.push_str("\r\n");
    out
}

/// Encode the per-target SLA table from an aggregate summary as a CSV
/// string. The first byte of the returned `Vec<u8>` is the UTF-8 BOM.
///
/// Column contract (do not reorder without bumping a doc version):
/// 1. target_id
/// 2. target_name
/// 3. window_start (ISO-8601 UTC)
/// 4. window_end (ISO-8601 UTC)
/// 5. window_seconds
/// 6. gross_uptime_percent (4 dp)
/// 7. sla_uptime_percent (4 dp)
/// 8. downtime_seconds
/// 9. excluded_seconds (subject 13, T-72: renamed from `maintenance_seconds`
///    -- this is specifically time excluded from the SLA denominator, not
///    "time in any maintenance window". Breaking change to I-08; see
///    CHANGELOG.md)
/// 10. incident_count
/// 11. mttr_seconds (empty when no resolved incidents)
/// 7. sla_uptime_percent is also empty, not `100.0000`, when the entire
///    window was excluded (FR-SLA-09) -- same convention as column 11.
pub fn encode_sla_summary(summary: &SlaSummary) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(UTF8_BOM);

    out.extend_from_slice(
        write_row(&[
            "target_id",
            "target_name",
            "window_start",
            "window_end",
            "window_seconds",
            "gross_uptime_percent",
            "sla_uptime_percent",
            "downtime_seconds",
            "excluded_seconds",
            "incident_count",
            "mttr_seconds",
        ])
        .as_bytes(),
    );

    for r in &summary.per_target {
        let mttr = match r.mttr_seconds {
            Some(s) => format_seconds(s),
            None => String::new(),
        };
        let row = write_row(&[
            &r.target_id,
            &r.target_name,
            &r.window_start,
            &r.window_end,
            &format_seconds(r.window_seconds),
            &format_ratio(r.gross_uptime_ratio),
            &format_ratio_opt(r.sla_uptime_ratio),
            &format_seconds(r.downtime_seconds),
            &format_seconds(r.excluded_seconds),
            &format_seconds(r.incident_count),
            &mttr,
        ]);
        out.extend_from_slice(row.as_bytes());
    }

    out
}

/// Encode a list of incidents as CSV. Used by the per-target detail page's
/// "Export incidents" button.
///
/// Column contract:
/// 1. incident_id
/// 2. target_id
/// 3. status
/// 4. opened_at
/// 5. resolved_at (empty for open incidents)
/// 6. duration_seconds (empty for open incidents)
/// 7. cause
/// 8. resolution_note
/// 9. created_by
pub fn encode_incidents(incidents: &[Incident]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(UTF8_BOM);

    out.extend_from_slice(
        write_row(&[
            "incident_id",
            "target_id",
            "status",
            "opened_at",
            "resolved_at",
            "duration_seconds",
            "cause",
            "resolution_note",
            "created_by",
        ])
        .as_bytes(),
    );

    for i in incidents {
        let resolved_at = i.resolved_at.as_deref().unwrap_or("");
        let duration = match i.duration_sec {
            Some(s) => format_seconds(s),
            None => String::new(),
        };
        let cause = i.cause.as_deref().unwrap_or("");
        let note = i.resolution_note.as_deref().unwrap_or("");
        let created_by = i.created_by.as_deref().unwrap_or("");
        let row = write_row(&[
            &i.id,
            &i.target_id,
            &i.status,
            &i.opened_at,
            resolved_at,
            &duration,
            cause,
            note,
            created_by,
        ]);
        out.extend_from_slice(row.as_bytes());
    }

    out
}

/// Build a download filename based on a kind label and the current UTC date.
/// Pure helper so the format is testable without touching the clock.
pub fn build_filename(kind: &str, date_yyyymmdd: &str) -> String {
    format!("noye-{}-{}.csv", kind, date_yyyymmdd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noye_shared::{SlaReport, SlaSummary};

    fn report(id: &str, name: &str) -> SlaReport {
        SlaReport {
            target_id: id.to_string(),
            target_name: name.to_string(),
            window_start: "2026-04-01T00:00:00Z".into(),
            window_end: "2026-04-02T00:00:00Z".into(),
            window_seconds: 86_400,
            downtime_seconds: 600,
            excluded_seconds: 0,
            gross_uptime_ratio: 0.99305,
            sla_uptime_ratio: Some(0.99305),
            incident_count: 1,
            mttr_seconds: Some(600),
        }
    }

    fn empty_summary() -> SlaSummary {
        SlaSummary {
            window_start: "2026-04-01T00:00:00Z".into(),
            window_end: "2026-04-02T00:00:00Z".into(),
            window_seconds: 86_400,
            per_target: Vec::new(),
            overall_gross_uptime_ratio: 1.0,
            overall_sla_uptime_ratio: Some(1.0),
        }
    }

    fn sample_incident(id: &str) -> Incident {
        Incident {
            id: id.to_string(),
            target_id: "t1".to_string(),
            status: "resolved".to_string(),
            opened_at: "2026-04-01T12:00:00Z".to_string(),
            resolved_at: Some("2026-04-01T12:10:00Z".to_string()),
            duration_sec: Some(600),
            cause: Some("connection refused".to_string()),
            resolution_note: Some("auto-resolved".to_string()),
            created_by: Some("system".to_string()),
        }
    }

    // ── quote_field ──

    #[test]
    fn unquoted_fields_pass_through() {
        assert_eq!(quote_field(""), "");
        assert_eq!(quote_field("simple"), "simple");
        assert_eq!(quote_field("with spaces"), "with spaces");
    }

    #[test]
    fn comma_triggers_quoting() {
        assert_eq!(quote_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn double_quote_is_doubled() {
        assert_eq!(quote_field(r#"he said "hi""#), r#""he said ""hi""""#);
    }

    #[test]
    fn newline_triggers_quoting() {
        assert_eq!(quote_field("line1\nline2"), "\"line1\nline2\"");
        assert_eq!(quote_field("line1\rline2"), "\"line1\rline2\"");
    }

    // ── format_ratio / format_seconds ──

    #[test]
    fn format_ratio_uses_four_decimal_places() {
        assert_eq!(format_ratio(1.0), "100.0000");
        assert_eq!(format_ratio(0.99999), "99.9990");
        assert_eq!(format_ratio(0.0), "0.0000");
        assert_eq!(format_ratio(0.5), "50.0000");
    }

    #[test]
    fn format_ratio_rounds_consistently() {
        // 99.99949% -> 99.9995 (banker's rounding via std default)
        let formatted = format_ratio(0.9999949);
        assert!(formatted.starts_with("99.9994") || formatted.starts_with("99.9995"));
    }

    #[test]
    fn format_seconds_is_plain_integer() {
        assert_eq!(format_seconds(0), "0");
        assert_eq!(format_seconds(86_400), "86400");
        assert_eq!(format_seconds(-1), "-1"); // defensive — shouldn't happen but shouldn't crash
    }

    // ── write_row ──

    #[test]
    fn rows_end_with_crlf_per_rfc_4180() {
        let row = write_row(&["a", "b"]);
        assert!(row.ends_with("\r\n"));
        assert_eq!(row, "a,b\r\n");
    }

    #[test]
    fn rows_quote_only_fields_that_need_it() {
        let row = write_row(&["plain", "has,comma", "with\"quote"]);
        assert_eq!(row, "plain,\"has,comma\",\"with\"\"quote\"\r\n");
    }

    // ── encode_sla_summary ──

    #[test]
    fn empty_summary_emits_bom_and_header_only() {
        let bytes = encode_sla_summary(&empty_summary());
        assert_eq!(&bytes[..3], UTF8_BOM, "expected UTF-8 BOM");
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.starts_with("target_id,target_name,window_start,window_end,window_seconds,"));
        assert!(text.ends_with("\r\n"));
        // No data rows
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn summary_emits_one_row_per_target() {
        let mut s = empty_summary();
        s.per_target = vec![report("t1", "API"), report("t2", "DB")];
        let bytes = encode_sla_summary(&s);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        // header + 2 rows
        assert_eq!(text.lines().count(), 3);
        assert!(text.contains("\r\nt1,API,"));
        assert!(text.contains("\r\nt2,DB,"));
    }

    #[test]
    fn summary_emits_empty_mttr_when_none() {
        let mut s = empty_summary();
        let mut r = report("t1", "API");
        r.mttr_seconds = None;
        s.per_target = vec![r];
        let bytes = encode_sla_summary(&s);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        // The trailing column is mttr_seconds. None -> empty -> the row
        // ends with ",\r\n".
        assert!(
            text.contains(",\r\n"),
            "row should end with empty trailing field"
        );
    }

    #[test]
    fn summary_quotes_target_names_with_commas() {
        let mut s = empty_summary();
        s.per_target = vec![report("t1", "Hello, World")];
        let bytes = encode_sla_summary(&s);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.contains("\"Hello, World\""));
    }

    #[test]
    fn summary_handles_unicode_target_names() {
        let mut s = empty_summary();
        s.per_target = vec![report("t1", "サーバー監視")];
        let bytes = encode_sla_summary(&s);
        // BOM is in place, the Japanese text should round-trip.
        assert_eq!(&bytes[..3], UTF8_BOM);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.contains("サーバー監視"));
    }

    // ── encode_incidents ──

    #[test]
    fn incidents_csv_starts_with_bom_and_header() {
        let bytes = encode_incidents(&[]);
        assert_eq!(&bytes[..3], UTF8_BOM);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.starts_with("incident_id,target_id,status,opened_at,resolved_at,"));
    }

    #[test]
    fn incidents_csv_includes_resolved_data() {
        let bytes = encode_incidents(&[sample_incident("inc-1")]);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.contains("inc-1,t1,resolved,2026-04-01T12:00:00Z,2026-04-01T12:10:00Z,600,"));
        assert!(text.contains("connection refused"));
    }

    #[test]
    fn incidents_csv_emits_blanks_for_open_incident() {
        let mut inc = sample_incident("inc-2");
        inc.status = "open".into();
        inc.resolved_at = None;
        inc.duration_sec = None;
        let bytes = encode_incidents(&[inc]);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        // Two consecutive empties for resolved_at and duration_seconds.
        assert!(text.contains("inc-2,t1,open,2026-04-01T12:00:00Z,,,"));
    }

    #[test]
    fn incidents_csv_quotes_cause_with_quotes_inside() {
        let mut inc = sample_incident("inc-3");
        inc.cause = Some(r#"got "503" from upstream"#.to_string());
        let bytes = encode_incidents(&[inc]);
        let text = std::str::from_utf8(&bytes[3..]).unwrap();
        assert!(text.contains(r#""got ""503"" from upstream""#));
    }

    // ── build_filename ──

    #[test]
    fn filename_uses_kind_and_date() {
        assert_eq!(build_filename("sla", "20260428"), "noye-sla-20260428.csv");
        assert_eq!(
            build_filename("incidents-t1", "20260428"),
            "noye-incidents-t1-20260428.csv"
        );
    }
}
