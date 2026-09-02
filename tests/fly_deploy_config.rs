//! Deployment-configuration guards.
//!
//! `tests/syntax_highlighting_backend.rs` holds the rest of them: the syntect
//! regex backend, the lockfile shape its C build depends on, and the builder
//! stage's C toolchain. If you are editing the `Dockerfile`, both files have
//! an opinion.

use autumn_web::config::{AutumnConfig, MockEnv};

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const DOCKERFILE: &str = include_str!("../Dockerfile");
const FLY_TOML: &str = include_str!("../fly.toml");
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
    assert!(CARGO_TOML.contains(r#"version = "0.7.0""#));
    assert!(EXPORT_RS.contains(r#"const AUTUMN_WEB_VERSION: &str = "0.7.0";"#));
}

#[test]
fn site_copy_targets_the_upcoming_autumn_docs_line() {
    assert!(SEO_RS.contains(r#"pub const AUTUMN_VERSION: &str = "0.7.0";"#));
    assert!(SITE_RS.contains(r#"const VERSION_LABEL: &str = "Autumn 0.7.0";"#));
    assert!(SEO_RS.contains(r#"pub const HARVEST_VERSION: &str = "0.6.0";"#));
}

/// `fly.toml` once set both `memory = '1gb'` and `memory_mb = 256` in the same
/// `[[vm]]` block. flyctl resolves that in `computeToGuest`: it fills the
/// guest's memory from `memory`, then copies the inlined `MachineGuest` — where
/// `memory_mb` is parsed — over it with `IgnoreEmpty`, so a non-zero
/// `memory_mb` silently wins. The file claimed 1 GB while the machine ran on
/// 256 MB.
///
/// The Fly dashboard writes to this file (see the `flyio-scale-from-ui`
/// commit), so the pair can come back. Keep exactly one memory key, and keep it
/// the documented one — `memory_mb` is undocumented legacy.
#[test]
fn fly_vm_declares_exactly_one_memory_key() {
    let memory_keys = FLY_TOML
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter(|line| {
            line.starts_with("memory")
                && line
                    .split('=')
                    .next()
                    .is_some_and(|key| key.trim() == "memory" || key.trim() == "memory_mb")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        memory_keys.len(),
        1,
        "fly.toml must declare exactly one memory key, found: {memory_keys:?}"
    );
    assert!(
        memory_keys[0].starts_with("memory ="),
        "use the documented `memory` key, not legacy `memory_mb`: {memory_keys:?}"
    );
}
