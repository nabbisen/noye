use super::*;
use std::io::Cursor;

const D1_INFO_FIXTURE: &str = include_str!("../fixtures/d1-info.json");

#[test]
fn t231_d1_id_parsed_from_captured_json_fixture() {
    let id = parse_d1_database_id(D1_INFO_FIXTURE).expect("fixture must parse");
    assert_eq!(id, "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a");
}

// ── T-232 (critical guard): an unparseable id response prompts rather
// than writing a guessed value. Proven as a must-fail-first pair: each
// case below is a shape a naive parser (e.g. "grab the first quoted
// 36-char-ish string anywhere in the output") would have accepted and
// silently used. `parse_d1_database_id` must reject every one of them
// as an `Err`, never an `Ok` carrying a guess.

#[test]
fn t232_missing_uuid_field_is_refused_not_guessed() {
    let err = parse_d1_database_id(r#"{"name": "noye_db", "num_tables": 3}"#)
        .expect_err("no uuid field present -- must not fabricate one");
    assert_eq!(err, IdResolutionError::MissingField("uuid"));
}

#[test]
fn t232_non_string_uuid_field_is_refused_not_guessed() {
    // A future wrangler version could plausibly nest this under an
    // object, or emit a number. Either way: not a string, not usable.
    let err = parse_d1_database_id(r#"{"uuid": 12345}"#)
        .expect_err("a non-string uuid must not be coerced into one");
    assert_eq!(err, IdResolutionError::MissingField("uuid"));
}

#[test]
fn t232_malformed_uuid_shape_is_refused_not_guessed() {
    let err = parse_d1_database_id(r#"{"uuid": "not-a-real-uuid"}"#)
        .expect_err("a wrong-shaped value under the right key must still be refused");
    assert_eq!(
        err,
        IdResolutionError::UnexpectedShape("not-a-real-uuid".to_string())
    );
}

#[test]
fn t232_truncated_json_is_refused_not_guessed() {
    // Simulates wrangler crashing mid-write, or a captured pipe getting
    // cut -- the output that reaches us is not JSON at all.
    let err = parse_d1_database_id(r#"{"uuid": "7b1cee56-8b93-4d3a-9e40-2b2b6a4"#)
        .expect_err("truncated JSON must not partially parse into a guess");
    assert!(matches!(err, IdResolutionError::NotJson(_)));
}

#[test]
fn t232_human_table_output_is_refused_not_scraped() {
    // The non---json path (the operator forgot --json, or a future
    // wrangler defaults it off) -- explicitly not something we scrape,
    // per the handoff's "do not scrape the human-readable table".
    let human_table = "┌──────────┬──────────────────────────────────────┐\n\
                        │ name     │ noye_db                                │\n\
                        └──────────┴──────────────────────────────────────┘";
    let err = parse_d1_database_id(human_table)
        .expect_err("a human-readable table must not be scraped for an id");
    assert!(matches!(err, IdResolutionError::NotJson(_)));
}

#[test]
fn looks_like_uuid_accepts_canonical_shape() {
    assert!(looks_like_uuid("7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a"));
}

#[test]
fn looks_like_uuid_rejects_wrong_length() {
    assert!(!looks_like_uuid("7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4"));
    assert!(!looks_like_uuid("7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4aa"));
}

#[test]
fn looks_like_uuid_rejects_wrong_hyphen_positions() {
    assert!(!looks_like_uuid("7b1cee568b93-4d3a-9e40-2b2b6a4d9f4a"));
}

#[test]
fn looks_like_uuid_rejects_non_hex_characters() {
    assert!(!looks_like_uuid("zzzzzzzz-8b93-4d3a-9e40-2b2b6a4d9f4a"));
}

// ── KV prompt fallback ──

#[test]
fn kv_prompt_returns_the_pasted_line_trimmed() {
    let input = Cursor::new(b"7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a\n".to_vec());
    let mut output = Vec::new();
    let id = prompt_for_kv_namespace_id("noye-cache-kv", input, &mut output)
        .expect("reading a line from an in-memory reader cannot fail");
    assert_eq!(id, "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a");
}

#[test]
fn kv_prompt_writes_the_namespace_name_into_its_own_prompt() {
    let input = Cursor::new(b"any-id\n".to_vec());
    let mut output = Vec::new();
    prompt_for_kv_namespace_id("noye-staging-cache-kv", input, &mut output).unwrap();
    let printed = String::from_utf8(output).unwrap();
    assert!(printed.contains("noye-staging-cache-kv"));
}

#[test]
fn kv_prompt_never_claims_to_have_json_it_does_not_have() {
    // Guards against someone "fixing" this later by quietly trying
    // --json and falling back only on failure -- the handoff is
    // explicit that this path must not scrape or guess at all.
    let input = Cursor::new(b"any-id\n".to_vec());
    let mut output = Vec::new();
    prompt_for_kv_namespace_id("ns", input, &mut output).unwrap();
    let printed = String::from_utf8(output).unwrap();
    assert!(printed.to_lowercase().contains("paste"));
}
