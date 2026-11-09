//! Atomic `wrangler.toml` writing (design point 6).
//!
//! `provision` never edits an existing config in place and never hand-
//! builds a TOML AST. It renders the committed `.example` template with
//! exact substring substitution, then writes the result to a temp
//! file, proves the temp file parses as TOML, and only then renames it
//! into place. Refuses to overwrite an existing config unless
//! `--force` -- someone's filled-in production config is not ours to
//! replace because a flag was mistyped.

use anyhow::{Context, Result, anyhow};
use std::path::Path;

/// Render `template` with every `(from, to)` pair applied via exact
/// substring replacement, in order. Refuses rather than silently doing
/// nothing when an anchor's occurrence count is not exactly 1.
///
/// `str::replace` on an absent needle returns the haystack unchanged
/// and reports nothing -- if a template is ever reformatted, a key
/// renamed, or an anchor mistyped, the substitution would quietly not
/// happen, the untouched text (a committed template, so always valid
/// TOML on its own) would still parse, and `write_config_atomic`'s
/// TOML check would pass it straight through to disk. Two of this
/// subject's own anchors (`OIDC_ISSUER_URL`/`OIDC_CLIENT_ID`'s
/// defaults) had no test asserting their placeholder was gone --
/// exactly the shape that reproduces G-48's symptom (a Gateway that
/// deploys and cannot be logged into) via a path other than the one
/// G-48 itself closed. Caught in review
/// (`.git-exclude/reviewed/079-subject-37a-ruling.md` §1) rather than
/// by a new per-anchor test, since the count check here covers every
/// anchor, including ones nobody has written a test for yet.
pub fn render(template: &str, substitutions: &[(&str, &str)]) -> Result<String> {
    let mut out = template.to_string();
    for (from, to) in substitutions {
        let n = out.matches(from).count();
        if n != 1 {
            return Err(anyhow!(
                "template substitution anchor {from:?} matched {n} times, expected exactly 1 \
                 -- the template and the renderer have drifted apart"
            ));
        }
        out = out.replace(from, to);
    }
    Ok(out)
}

/// Write `contents` to `target` atomically. Refuses when `target`
/// already exists unless `force` is set. Validates `contents` parses as
/// TOML *before* touching `target`'s path at all, then re-validates
/// what actually landed on disk (not just the in-memory string) before
/// the rename, so a filesystem-level surprise -- partial write, odd
/// encoding -- is caught rather than silently renamed into place.
pub fn write_config_atomic(target: &Path, contents: &str, force: bool) -> Result<()> {
    if target.exists() && !force {
        return Err(anyhow!(
            "{} already exists -- refusing to overwrite without --force",
            target.display()
        ));
    }

    toml::from_str::<toml::Value>(contents).with_context(|| {
        format!(
            "generated config for {} is not valid TOML -- refusing to write it",
            target.display()
        )
    })?;

    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("wrangler.toml");
    let tmp_path = dir.join(format!(".{file_name}.tmp-{}", std::process::id()));

    std::fs::write(&tmp_path, contents)
        .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;

    let written = std::fs::read_to_string(&tmp_path)
        .with_context(|| format!("failed to read back {}", tmp_path.display()))?;
    if let Err(e) = toml::from_str::<toml::Value>(&written) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(anyhow!(
            "the file written to disk at {} did not re-parse as TOML ({e}) -- removed, target untouched",
            tmp_path.display()
        ));
    }

    std::fs::rename(&tmp_path, target)
        .with_context(|| format!("failed to rename {} into place", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests;
