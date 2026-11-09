use super::*;

#[test]
fn t236_a_workers_dev_host_resolves_to_workers_dev_reachability() {
    let resolved = resolve(
        Some("noye-verify-gateway.scqr-inc.workers.dev"),
        "noye-verify-gateway",
    )
    .expect("a matching workers.dev host must resolve");
    assert_eq!(resolved.reachability, Reachability::WorkersDev);
}

#[test]
fn t237_a_custom_domain_host_resolves_to_custom_domain_reachability() {
    let resolved =
        resolve(Some("noye.example.com"), "noye-gateway").expect("a custom domain must resolve");
    assert_eq!(resolved.reachability, Reachability::CustomDomain);
}

#[test]
fn t238_redirect_uri_is_rendered_from_the_hostname_in_both_shapes() {
    let workers_dev = resolve(Some("noye-gateway.acct.workers.dev"), "noye-gateway").unwrap();
    assert_eq!(
        workers_dev.redirect_uri,
        "https://noye-gateway.acct.workers.dev/auth/callback"
    );

    let custom = resolve(Some("noye.scqr.net"), "noye-gateway").unwrap();
    assert_eq!(custom.redirect_uri, "https://noye.scqr.net/auth/callback");
}

#[test]
fn t238_localhost_never_appears_in_a_resolved_redirect_uri() {
    for host in ["noye-gateway.acct.workers.dev", "noye.scqr.net"] {
        let resolved = resolve(Some(host), "noye-gateway").unwrap();
        assert!(!resolved.redirect_uri.contains("localhost"));
    }
}

// ── T-239 (critical guard): a workers.dev host whose first label does
// not match the derived Gateway worker name is refused, naming both.

#[test]
fn t239_mismatched_workers_dev_label_is_refused() {
    let err = resolve(
        Some("noye-gateway.scqr-inc.workers.dev"),
        "noye-verify-gateway",
    )
    .expect_err("a label that doesn't match the derived worker name must be refused");
    assert_eq!(
        err,
        HostnameError::WorkersDevLabelMismatch {
            host: "noye-gateway.scqr-inc.workers.dev".to_string(),
            expected_label: "noye-verify-gateway".to_string(),
            found_label: "noye-gateway".to_string(),
        }
    );
}

#[test]
fn t239_the_refusal_message_names_both_the_expected_and_found_label() {
    let err = resolve(
        Some("noye-gateway.scqr-inc.workers.dev"),
        "noye-verify-gateway",
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("noye-verify-gateway"),
        "must name the expected label"
    );
    assert!(
        message.contains("noye-gateway"),
        "must name the found label"
    );
}

#[test]
fn t239_a_matching_workers_dev_label_is_accepted() {
    // Guard against the fix overcorrecting into refusing everything.
    resolve(
        Some("noye-verify-gateway.scqr-inc.workers.dev"),
        "noye-verify-gateway",
    )
    .expect("a label that matches the derived worker name must be accepted");
}

// ── T-240 (critical guard): missing --gateway-hostname is refused, not
// defaulted. Pure-function proof that this fails before any step could
// possibly run -- `resolve` spawns no subprocess and touches no
// filesystem, so a caller that `?`-propagates this error (as
// `provision_run` does, before constructing a `Plan` or a step list)
// cannot reach any step.

#[test]
fn t240_missing_hostname_is_refused_not_defaulted() {
    let err = resolve(None, "noye-gateway").expect_err("a missing hostname must be refused");
    assert_eq!(err, HostnameError::Missing);
}

#[test]
fn t240_the_missing_message_names_both_valid_forms() {
    let message = resolve(None, "noye-gateway").unwrap_err().to_string();
    assert!(message.contains("workers.dev"));
    assert!(message.to_lowercase().contains("custom domain"));
}

#[test]
fn single_label_workers_dev_host_does_not_panic() {
    // Degenerate but not malformed: no account subdomain between the
    // worker name and .workers.dev.
    let resolved = resolve(Some("noye-gateway.workers.dev"), "noye-gateway").unwrap();
    assert_eq!(resolved.reachability, Reachability::WorkersDev);
}
