//! Resolving generated resource ids without hand-copying (G-45).
//!
//! Only two resources need an id resolved after creation: D1's
//! `database_id` and a KV namespace's `id`. R2 buckets are referenced
//! by name only in `wrangler.toml` and need no id at all.
//!
//! Both paths refuse to guess. A wrangler version change must degrade
//! to a prompt, not to a corrupted config (subject 37, design point 5).

use std::fmt;
use std::io::{BufRead, Write};

#[derive(Debug, PartialEq, Eq)]
pub enum IdResolutionError {
    NotJson(String),
    MissingField(&'static str),
    UnexpectedShape(String),
}

impl fmt::Display for IdResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdResolutionError::NotJson(e) => write!(f, "not valid JSON: {e}"),
            IdResolutionError::MissingField(field) => {
                write!(f, "expected a \"{field}\" field, found none")
            }
            IdResolutionError::UnexpectedShape(got) => {
                write!(f, "\"uuid\" field did not look like a UUID: {got:?}")
            }
        }
    }
}

/// A UUID-shaped string: 36 characters, hyphens at positions 8/13/18/23,
/// hex digits everywhere else. We validate the *shape*, not that it is
/// a real Cloudflare id -- the goal is catching "wrangler changed its
/// output" before it becomes a silently wrong config, not verifying
/// provenance.
fn looks_like_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Parse `wrangler d1 info <name> --json`'s stdout for the database id.
///
/// Field name (`"uuid"`) confirmed by reading wrangler 4.97.0's own
/// bundled source locally (`getDatabaseInfoFromIdOrName` in
/// `src/d1/utils.ts`, which returns the Cloudflare D1 API's response
/// verbatim under `--json`, and the non-JSON table-rendering path,
/// which explicitly filters `"uuid"` out of the human table -- meaning
/// it exists in the JSON object it filters *from*) -- not observed
/// against a real account, since standing rule 7 puts that outside the
/// dev team's reach. See `provision/fixtures/d1-info.json`'s own header
/// for the same caveat.
pub fn parse_d1_database_id(json_stdout: &str) -> Result<String, IdResolutionError> {
    let value: serde_json::Value =
        serde_json::from_str(json_stdout).map_err(|e| IdResolutionError::NotJson(e.to_string()))?;
    let uuid = value
        .get("uuid")
        .and_then(|v| v.as_str())
        .ok_or(IdResolutionError::MissingField("uuid"))?;
    if !looks_like_uuid(uuid) {
        return Err(IdResolutionError::UnexpectedShape(uuid.to_string()));
    }
    Ok(uuid.to_string())
}

/// KV namespace id has no documented machine-readable output on either
/// `wrangler kv namespace create` or `wrangler kv namespace list`
/// (wrangler 4.97.0 `--help`, checked locally -- `d1 info`/`d1 create`
/// both document `--json`; neither KV command does). Per the handoff's
/// escalation: do not guess, and do not scrape the human-readable
/// table. Prompt instead, injecting the reader/writer so this is
/// testable without real stdin/stdout.
pub fn prompt_for_kv_namespace_id<R: BufRead, W: Write>(
    namespace_name: &str,
    mut reader: R,
    mut writer: W,
) -> std::io::Result<String> {
    write!(
        writer,
        "Could not resolve the KV namespace id for '{namespace_name}' automatically \
         (wrangler documents no machine-readable output for this). \
         Paste the id shown above: "
    )?;
    writer.flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// Interactive entry point over real stdin/stdout.
pub fn prompt_for_kv_namespace_id_interactive(namespace_name: &str) -> std::io::Result<String> {
    let stdin = std::io::stdin();
    prompt_for_kv_namespace_id(namespace_name, stdin.lock(), std::io::stdout())
}

#[cfg(test)]
mod tests;
