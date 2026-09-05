use super::*;
use std::fs;

/// A fresh, empty scratch directory per test -- no `tempfile` crate
/// dependency needed; `uuid` is already pulled in for the CLI's other
/// commands.
struct ScratchDir(std::path::PathBuf);

impl ScratchDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("noye-provision-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create scratch dir");
        ScratchDir(path)
    }

    fn join(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn render_applies_every_substitution() {
    let template = "name = \"noye-core\"\ndatabase_name = \"noye_db\"\n";
    let out = render(
        template,
        &[
            (r#"name = "noye-core""#, r#"name = "noye-staging-core""#),
            (
                r#"database_name = "noye_db""#,
                r#"database_name = "noye_staging_db""#,
            ),
        ],
    );
    assert_eq!(
        out,
        "name = \"noye-staging-core\"\ndatabase_name = \"noye_staging_db\"\n"
    );
}

#[test]
fn render_with_no_substitutions_is_the_identity() {
    let template = "name = \"noye-core\"\n";
    assert_eq!(render(template, &[]), template);
}

#[test]
fn t233_writes_when_target_does_not_exist() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");
    write_config_atomic(&target, "name = \"noye-core\"\n", false)
        .expect("first write must succeed");
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "name = \"noye-core\"\n"
    );
}

#[test]
fn t233_refuses_an_existing_target_without_force() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");
    fs::write(&target, "name = \"someones-real-config\"\n").unwrap();

    let result = write_config_atomic(&target, "name = \"noye-core\"\n", false);
    assert!(result.is_err(), "must refuse to overwrite without --force");

    // The one property that matters most: the existing file is
    // byte-for-byte untouched by the refused write.
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "name = \"someones-real-config\"\n"
    );
}

#[test]
fn t233_force_overwrites_an_existing_target() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");
    fs::write(&target, "name = \"old\"\n").unwrap();

    write_config_atomic(&target, "name = \"new\"\n", true).expect("force must overwrite");
    assert_eq!(fs::read_to_string(&target).unwrap(), "name = \"new\"\n");
}

#[test]
fn t233_invalid_toml_is_refused_and_writes_no_target_and_no_leftover_temp_file() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");

    // Deliberately broken TOML (unterminated string).
    let result = write_config_atomic(&target, "name = \"unterminated\n", false);
    assert!(
        result.is_err(),
        "invalid TOML must be refused before writing"
    );
    assert!(
        !target.exists(),
        "target must not be created from invalid content"
    );

    let leftovers: Vec<_> = fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temp file should survive a refused write, found: {leftovers:?}"
    );
}

#[test]
fn t233_a_refused_overwrite_leaves_no_leftover_temp_file_either() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");
    fs::write(&target, "name = \"existing\"\n").unwrap();

    write_config_atomic(&target, "name = \"attempted-overwrite\"\n", false).unwrap_err();

    let entries: Vec<_> = fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        entries,
        vec![std::ffi::OsString::from("wrangler.toml")],
        "only the original target file should remain -- no stray temp file"
    );
}

#[test]
fn t233_a_successful_write_leaves_no_leftover_temp_file() {
    let dir = ScratchDir::new();
    let target = dir.join("wrangler.toml");
    write_config_atomic(&target, "name = \"noye-core\"\n", false).unwrap();

    let entries: Vec<_> = fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("wrangler.toml")]);
}
