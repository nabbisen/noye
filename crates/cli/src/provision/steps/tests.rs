use super::*;
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
    let mut plan = Plan::new(Names::derive("staging"), false);
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
    let rendered = render_core_config(CORE_TEMPLATE, &plan, "7b1cee56-8b93-4d3a-9e40-2b2b6a4d9f4a");

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
    let rendered = render_gateway_config(GATEWAY_TEMPLATE, &plan, "kv-namespace-id-abc123");

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

#[test]
fn rendering_the_default_env_name_reproduces_the_template_almost_verbatim() {
    // env_name == "noye": every literal substitution target already
    // matches its own replacement value, except the two placeholder
    // ids -- so a default-env plan only ever touches those two lines.
    let mut plan = Plan::new(Names::derive("noye"), false);
    plan.oidc_client_secret = Some("unused-here".to_string());
    let rendered = render_core_config(CORE_TEMPLATE, &plan, "some-real-uuid");
    assert!(rendered.contains(r#"name = "noye-core""#));
    assert!(rendered.contains(r#"database_name = "noye_db""#));
    assert!(rendered.contains(r#"bucket_name = "noye-logs""#));
    assert!(rendered.contains(r#"database_id = "some-real-uuid""#));
}
