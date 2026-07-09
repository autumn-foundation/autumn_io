use autumn_web::config::{AutumnConfig, MockEnv};

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const DOCKERFILE: &str = include_str!("../Dockerfile");
const EXPORT_RS: &str = include_str!("../src/export.rs");
const SEO_RS: &str = include_str!("../src/seo.rs");
const SITE_RS: &str = include_str!("../src/site.rs");

#[test]
fn prod_profile_binds_to_all_interfaces_for_fly() {
    let env = MockEnv::new()
        .with("AUTUMN_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
        .with("AUTUMN_PROFILE", "prod");

    let config = AutumnConfig::load_with_env(&env).expect("prod config should load");

    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 3000);
}

#[test]
fn fly_runtime_exports_cargo_metadata_for_actuator_info() {
    assert!(DOCKERFILE.contains("ENV CARGO_PKG_NAME=autumn_io"));
    assert!(DOCKERFILE.contains("ENV CARGO_PKG_VERSION=0.1.0"));
}

#[test]
fn docker_build_uses_the_committed_dependency_lockfile() {
    assert!(DOCKERFILE.contains("COPY Cargo.toml Cargo.lock"));
    assert!(DOCKERFILE.contains("cargo build --locked --release --bin autumn_io"));
}

#[test]
fn runtime_versions_reflect_current_published_autumn_dependency() {
    assert!(CARGO_TOML.contains("autumn-web"));
    assert!(CARGO_TOML.contains(r#"version = "0.5.0""#));
    assert!(EXPORT_RS.contains(r#"const AUTUMN_WEB_VERSION: &str = "0.5.0";"#));
}

#[test]
fn site_copy_targets_the_upcoming_autumn_docs_line() {
    assert!(SEO_RS.contains(r#"pub const AUTUMN_VERSION: &str = "0.5.0";"#));
    assert!(SITE_RS.contains(r#"const VERSION_LABEL: &str = "Autumn 0.5.0";"#));
}
