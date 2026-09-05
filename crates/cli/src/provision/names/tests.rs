use super::*;

#[test]
fn t228_default_env_name_reproduces_the_literal_defaults() {
    let names = Names::derive("noye");
    assert_eq!(names.core_worker, "noye-core");
    assert_eq!(names.gateway_worker, "noye-gateway");
    assert_eq!(names.d1_database, "noye_db");
    assert_eq!(names.r2_bucket, "noye-logs");
}

#[test]
fn t228_staging_env_name_derives_all_four_names_correctly() {
    let names = Names::derive("staging");
    assert_eq!(names.core_worker, "noye-staging-core");
    assert_eq!(names.gateway_worker, "noye-staging-gateway");
    assert_eq!(names.d1_database, "noye_staging_db");
    assert_eq!(names.r2_bucket, "noye-staging-logs");
}

#[test]
fn derived_names_never_double_the_noye_prefix() {
    let names = Names::derive("noye");
    assert!(!names.core_worker.contains("noye-noye"));
    assert!(!names.d1_database.contains("noye_noye"));
}

#[test]
fn two_different_env_names_never_collide_on_any_derived_name() {
    let a = Names::derive("staging");
    let b = Names::derive("production");
    assert_ne!(a.core_worker, b.core_worker);
    assert_ne!(a.gateway_worker, b.gateway_worker);
    assert_ne!(a.d1_database, b.d1_database);
    assert_ne!(a.r2_bucket, b.r2_bucket);
}

#[test]
fn env_name_is_preserved_verbatim_on_the_struct() {
    assert_eq!(Names::derive("acme").env_name, "acme");
}
