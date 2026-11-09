use super::*;
use crate::provision::gateway_hostname;
use crate::provision::names::Names;

// The real, committed templates -- `include_str!` so these tests catch
// a template edit that breaks provision's substitutions immediately,
// without needing the workspace root as CWD.
const CORE_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../core/wrangler.toml.example"
));
const GATEWAY_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../gateway/wrangler.toml.example"
));

fn test_plan() -> Plan {
    let names = Names::derive("staging");
    let gateway_hostname =
        gateway_hostname::resolve(Some("noye.example.com"), &names.gateway_worker)
            .expect("a custom domain always resolves");
    let mut plan = Plan::new(names, false);
    plan.gateway_hostname = Some(gateway_hostname);
    plan.d1_database_id = Some("7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a".to_string());
    plan.kv_namespace_id = Some("kv-id-placeholder".to_string());
    plan.oidc_client_secret = Some("oidc-secret-should-never-appear".to_string());
    plan.admin_email = Some("admin@example.com".to_string());
    plan.admin_name = Some("Admin".to_string());
    plan
}

// ── T-230 (critical guard): the generated shared secret appears in no
// plan output. Proven must-fail-first: `Leaky` below deliberately
// embeds the secret in `describe()`, and the sanity test confirms the
// scan actually catches it, before the real assertion trusts the same
// scan against every real step.

struct Leaky;
impl Step for Leaky {
    fn name(&self) -> &'static str {
        "leaky"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "secret is {}",
            plan.shared_secret.as_deref().unwrap_or("<unset>")
        )
    }
    fn apply(&self, _plan: &mut Plan) -> Result<()> {
        Ok(())
    }
}

#[test]
fn t230_sanity_a_leaky_step_is_actually_caught_by_the_scan() {
    let mut plan = test_plan();
    plan.shared_secret = Some("leaked-value-xyz".to_string());
    let output = Leaky.describe(&plan);
    assert!(
        output.contains("leaked-value-xyz"),
        "the sanity fixture must actually leak, or the real assertion below proves nothing"
    );
}

#[test]
fn t230_no_real_step_describe_output_contains_the_shared_secret() {
    let mut plan = test_plan();
    plan.shared_secret = Some("super-secret-should-never-appear-anywhere".to_string());
    for step in default_steps() {
        let output = step.describe(&plan);
        assert!(
            !output.contains("super-secret-should-never-appear-anywhere"),
            "step '{}' leaked the shared secret in describe()",
            step.name()
        );
    }
}

#[test]
fn t230_no_real_step_describe_output_contains_the_oidc_client_secret() {
    let plan = test_plan();
    for step in default_steps() {
        let output = step.describe(&plan);
        assert!(
            !output.contains("oidc-secret-should-never-appear"),
            "step '{}' leaked the OIDC client secret in describe()",
            step.name()
        );
    }
}

#[test]
fn t230_describe_never_leaks_the_secret_even_before_it_is_generated() {
    // shared_secret is None at --plan time (nothing has been generated
    // yet). Naming the *env var* (GATEWAY_SHARED_TOKEN) in the plan
    // text is fine and expected -- describing what a step does is the
    // point of --plan. What must never appear is a fabricated stand-in
    // for the secret's future *value*.
    let plan = test_plan();
    assert!(plan.shared_secret.is_none());
    for step in default_steps() {
        let output = step.describe(&plan);
        assert!(
            !output.contains("Some(") && !output.contains("None"),
            "step '{}' leaked a raw Option<String> debug rendering instead of a human description",
            step.name()
        );
    }
}

// ── describe() sanity: every step's plan text uses the derived names,
// not the hardcoded defaults, for a non-default --env-name.

#[test]
fn describe_uses_derived_names_not_hardcoded_defaults() {
    let plan = test_plan(); // env_name = "staging"
    let combined: String = default_steps()
        .iter()
        .map(|s| s.describe(&plan))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(combined.contains("noye-staging-core"));
    assert!(combined.contains("noye-staging-gateway"));
    assert!(combined.contains("noye_staging_db"));
    assert!(combined.contains("noye-staging-logs"));
    assert!(
        !combined.contains("\"noye-core\""),
        "the literal default name must not leak into a staging plan's description"
    );
}

#[test]
fn describe_never_panics_on_a_plan_with_unset_resolved_fields() {
    // At --plan time nothing is resolved yet. describe() must survive
    // that (used to print the *plan*, not the outcome).
    let plan = Plan::new(Names::derive("noye"), false);
    for step in default_steps() {
        let _ = step.describe(&plan); // must not panic
    }
}

// ── Deploy ordering (design point 9) ──

#[test]
fn deploy_gateway_refuses_before_deploy_core_has_completed() {
    let mut plan = test_plan();
    assert!(!plan.completed.contains(&"deploy_core"));
    let result = DeployGateway.apply(&mut plan);
    assert!(
        result.is_err(),
        "Gateway must refuse to deploy before Core has completed"
    );
}

// ── Rendering against the real, committed templates -- not synthetic
// strings. Catches a future `.example` edit that breaks provision's
// substitutions (a literal renamed, re-quoted, or removed) immediately,
// rather than only at the next real deployment session.

#[test]
fn render_core_config_against_the_real_template_produces_valid_toml() {
    let plan = test_plan();
    let rendered = render_core_config(CORE_TEMPLATE, &plan, "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a")
        .expect("every anchor must match exactly once in the real template");

    toml::from_str::<toml::Value>(&rendered).expect("rendered core config must be valid TOML");
    assert!(rendered.contains(r#"name = "noye-staging-core""#));
    assert!(rendered.contains(r#"database_name = "noye_staging_db""#));
    assert!(rendered.contains(r#"database_id = "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a""#));
    assert!(rendered.contains(r#"bucket_name = "noye-staging-logs""#));
    assert!(
        !rendered.contains("YOUR_DATABASE_ID_HERE"),
        "the placeholder must be fully replaced"
    );
    assert!(
        !rendered.contains(r#"name = "noye-core""#),
        "the literal default worker name must not survive rendering for a non-default env"
    );
}

#[test]
fn render_gateway_config_against_the_real_template_produces_valid_toml() {
    let mut plan = test_plan();
    plan.oidc_issuer_url = Some("https://idp.example.com".to_string());
    plan.oidc_client_id = Some("noye-staging-client".to_string());
    let hostname = plan.gateway_hostname.clone().unwrap();
    let rendered =
        render_gateway_config(GATEWAY_TEMPLATE, &plan, "kv-namespace-id-abc123", &hostname)
            .expect("every anchor must match exactly once in the real template");

    toml::from_str::<toml::Value>(&rendered).expect("rendered gateway config must be valid TOML");
    assert!(rendered.contains(r#"name = "noye-staging-gateway""#));
    assert!(rendered.contains(r#"id = "kv-namespace-id-abc123""#));
    assert!(
        rendered.contains(r#"service = "noye-staging-core""#),
        "the Service Binding must follow Core's own derived name, not the literal default"
    );
    assert!(rendered.contains(r#"OIDC_ISSUER_URL = "https://idp.example.com""#));
    assert!(rendered.contains(r#"OIDC_CLIENT_ID = "noye-staging-client""#));
    assert!(!rendered.contains("YOUR_KV_ID_HERE"));
    assert!(
        !rendered.contains(r#"service = "noye-core""#),
        "the literal default service binding must not survive rendering for a non-default env"
    );
}

// ── T-236/T-237/T-238/T-242: the reachability mechanism, rendered
// against the real template.

#[test]
fn t236_workers_dev_host_sets_workers_dev_true_and_writes_no_route_block() {
    let plan = test_plan();
    let hostname = gateway_hostname::resolve(
        Some("noye-staging-gateway.scqr-inc.workers.dev"),
        &plan.names.gateway_worker,
    )
    .unwrap();
    let rendered = render_gateway_config(GATEWAY_TEMPLATE, &plan, "kv-id", &hostname)
        .expect("every anchor must match exactly once in the real template");

    toml::from_str::<toml::Value>(&rendered).expect("must still be valid TOML");
    // Anchored on a leading newline: the explanatory comment two lines
    // above the real line also says "workers_dev = true" (backtick-
    // quoted, describing the local-dev override), so the bare
    // substring appears in the template regardless of whether the
    // real line was ever touched -- this is exactly the assertion
    // that failed to fail when it should have, caught while writing
    // this test (see steps.rs's `workers_dev_block` doc comment).
    assert!(rendered.contains("\nworkers_dev = true"));
    assert!(
        !rendered.contains("\nworkers_dev = false"),
        "the real line must not still read false"
    );
    assert!(
        // Anchored for the same reason as above: the explanatory
        // comment block right above `workers_dev` prose-describes the
        // `[[routes]]` table this subject can add, so the bare
        // substring is present in the template text regardless of
        // whether a real table was ever written. The real table, if
        // present, always starts a fresh line.
        !rendered.contains("\n[[routes]]"),
        "a workers.dev host must not get a route block"
    );
    assert!(rendered.contains(
        r#"OIDC_REDIRECT_URI = "https://noye-staging-gateway.scqr-inc.workers.dev/auth/callback""#
    ));
}

#[test]
fn t237_custom_domain_host_sets_workers_dev_false_and_writes_a_route_block() {
    let plan = test_plan();
    let hostname =
        gateway_hostname::resolve(Some("noye.scqr.net"), &plan.names.gateway_worker).unwrap();
    let rendered = render_gateway_config(GATEWAY_TEMPLATE, &plan, "kv-id", &hostname)
        .expect("every anchor must match exactly once in the real template");

    toml::from_str::<toml::Value>(&rendered).expect("must still be valid TOML");
    assert!(rendered.contains("\nworkers_dev = false"));
    assert!(rendered.contains("\n[[routes]]\n"));
    assert!(rendered.contains(r#"pattern = "noye.scqr.net""#));
    assert!(rendered.contains("custom_domain = true"));
    assert!(rendered.contains(r#"OIDC_REDIRECT_URI = "https://noye.scqr.net/auth/callback""#));
}

#[test]
fn t238_localhost_8787_survives_rendering_in_neither_shape() {
    let plan = test_plan();
    for host in ["noye-staging-gateway.scqr-inc.workers.dev", "noye.scqr.net"] {
        let hostname = gateway_hostname::resolve(Some(host), &plan.names.gateway_worker).unwrap();
        let rendered = render_gateway_config(GATEWAY_TEMPLATE, &plan, "kv-id", &hostname)
            .expect("every anchor must match exactly once in the real template");
        assert!(
            !rendered.contains("localhost:8787"),
            "localhost:8787 survived rendering for host '{host}'"
        );
    }
}

#[test]
fn t242_core_config_still_has_workers_dev_false_and_no_route_regardless_of_gateway_hostname() {
    // The one property that protects the architecture: a fix aimed at
    // the Gateway must never reach Core. Core's unroutability is the
    // external design's single most important external property.
    // render_core_config doesn't even take a hostname parameter -- this
    // asserts the real template's own invariant stays true regardless.
    let plan = test_plan();
    let rendered = render_core_config(CORE_TEMPLATE, &plan, "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a")
        .expect("every anchor must match exactly once in the real template");
    assert!(rendered.contains("\nworkers_dev = false"));
    assert!(
        !rendered.contains("\n[[routes]]"),
        "Core must never gain a route block"
    );
}

#[test]
fn rendering_the_default_env_name_reproduces_the_template_almost_verbatim() {
    // env_name == "noye": every literal substitution target already
    // matches its own replacement value, except the two placeholder
    // ids -- so a default-env plan only ever touches those two lines.
    let mut plan = Plan::new(Names::derive("noye"), false);
    plan.oidc_client_secret = Some("unused-here".to_string());
    let rendered = render_core_config(CORE_TEMPLATE, &plan, "some-real-uuid")
        .expect("every anchor must match exactly once in the real template");
    assert!(rendered.contains(r#"name = "noye-core""#));
    assert!(rendered.contains(r#"database_name = "noye_db""#));
    assert!(rendered.contains(r#"bucket_name = "noye-logs""#));
    assert!(rendered.contains(r#"database_id = "some-real-uuid""#));
}

// ── Running a step out of order fails closed with a clear message,
// not a panic on an unwrap (mod.rs's `Plan` doc comment names this
// property; `require()` in steps.rs is what enforces it).

#[test]
fn write_core_config_run_before_resolve_d1_id_fails_closed_not_panics() {
    let mut plan = test_plan();
    plan.d1_database_id = None; // ResolveD1Id's step never ran
    let result = WriteCoreConfig.apply(&mut plan);
    let err = result.expect_err("must fail, not panic, when run out of order");
    assert!(err.to_string().contains("D1 database id"));
}

#[test]
fn write_gateway_config_run_before_resolve_kv_id_fails_closed_not_panics() {
    let mut plan = test_plan();
    plan.kv_namespace_id = None; // ResolveKvId's step never ran
    let result = WriteGatewayConfig.apply(&mut plan);
    let err = result.expect_err("must fail, not panic, when run out of order");
    assert!(err.to_string().contains("KV namespace id"));
}

#[test]
fn write_gateway_config_with_no_resolved_hostname_fails_closed_not_panics() {
    // Defensive: `provision_run` always resolves --gateway-hostname
    // before constructing any step, so this should never happen in
    // practice -- but `WriteGatewayConfig::apply` must still fail
    // closed, not panic on an unwrap, if it somehow did.
    let mut plan = test_plan();
    plan.gateway_hostname = None;
    let result = WriteGatewayConfig.apply(&mut plan);
    let err = result.expect_err("must fail, not panic, when the hostname was never resolved");
    assert!(err.to_string().contains("hostname"));
}

// ── T-241 (guard): --plan prints hostname, mechanism, and redirect URI.

#[test]
fn t241_describe_includes_hostname_mechanism_and_redirect_uri() {
    let plan = test_plan(); // gateway_hostname = noye.example.com (custom domain)
    let description = WriteGatewayConfig.describe(&plan);
    assert!(description.contains("noye.example.com"));
    assert!(description.contains("workers_dev = false"));
    assert!(description.contains("https://noye.example.com/auth/callback"));
}

#[test]
fn t241_describe_reflects_the_workers_dev_mechanism_too() {
    let mut plan = test_plan();
    plan.gateway_hostname = Some(
        gateway_hostname::resolve(
            Some("noye-staging-gateway.scqr-inc.workers.dev"),
            &plan.names.gateway_worker,
        )
        .unwrap(),
    );
    let description = WriteGatewayConfig.describe(&plan);
    assert!(description.contains("workers_dev = true"));
}
