use autumn_web::config::{AutumnConfig, MockEnv};

#[test]
fn prod_profile_binds_to_all_interfaces_for_fly() {
    let env = MockEnv::new()
        .with("AUTUMN_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
        .with("AUTUMN_PROFILE", "prod");

    let config = AutumnConfig::load_with_env(&env).expect("prod config should load");

    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 3000);
}
