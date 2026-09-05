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

/// The settings of the `[profile.release]` block, without its comments.
///
/// Scoped to the one block so that a `[profile.dev]` or a dependency line
/// containing the same words cannot satisfy — or break — a check below by
/// accident. Comments are dropped for the same reason they are in
/// `dockerfile_builder_keeps_the_c_toolchain_oniguruma_needs`: that block names
/// `panic = "abort"` and `strip` explicitly in order to warn against them, and
/// the assertions here are about what the profile *sets*.
fn release_profile() -> String {
    let (_, profile) = CARGO_TOML
        .split_once("[profile.release]")
        .expect("Cargo.toml should declare a release profile");

    profile
        .split_once("\n[")
        .map_or(profile, |(section, _)| section)
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The binary is built once per deploy and then serves until the next one, so
/// the profile is tuned for what runs rather than for build time.
///
/// `opt-level` is the one worth stating a reason for: the size levels are a
/// standing temptation and both were measured and rejected. `"s"` costs 11% of
/// the cold-start render and `"z"` costs 27%, against a corpus this app renders
/// on the first request after every scale-to-zero.
#[test]
fn release_profile_optimises_the_deployed_binary() {
    let profile = release_profile();

    assert!(
        profile.contains("opt-level = 3"),
        "opt-level 3: \"s\" and \"z\" were measured at +11.0% and +26.7% on the \
         cold-start render (see docs/plans/2026-09-04-release-profile-and-ci.md)"
    );
    assert!(profile.contains(r#"lto = "fat""#));
    assert!(profile.contains("codegen-units = 1"));
}

/// `panic = "abort"` is the standard companion to a profile like this one and
/// is wrong for this app.
///
/// Autumn treats unwinding as a correctness mechanism: `autumn-web`'s `db.rs`
/// catches panics inside a transaction because one that unwound without a
/// rollback would let deadpool recycle a connection with an open write
/// transaction, and the job runner and event dispatcher isolate a panicking
/// handler from its siblings the same way. Aborting converts each of those
/// boundaries into a process kill.
#[test]
fn release_profile_keeps_unwinding_for_autumn_panic_isolation() {
    let profile = release_profile();

    assert!(
        !profile.contains(r#"panic = "abort""#),
        "autumn-web catches panics to roll transactions back and to isolate \
         jobs and event handlers; aborting kills the process instead"
    );
    assert!(
        profile.contains(r#"panic = "unwind""#),
        "state it explicitly, so the choice reads as deliberate"
    );
}

/// Fly's built-in Prometheus scraping needs to be told where the scrape
/// endpoint lives. Without this block Fly never learns about
/// `/actuator/prometheus`, and the `fly-autoscaler` companion app
/// (`fly-autoscaler/fly.toml`) has nothing to query — see
/// `fly-autoscaler/README.md`.
#[test]
fn fly_toml_exposes_the_prometheus_scrape_endpoint() {
    assert!(FLY_TOML.contains("[metrics]"));
    assert!(FLY_TOML.contains(r#"path = "/actuator/prometheus""#));
}

/// Stripping is the largest remaining size lever and it is declined.
///
/// `src/bin/profile_docs_search.rs` already warns about it in prose —
/// callgrind attributes by symbol, and the figures in the last three plan
/// documents were taken that way — and it also turns production panic
/// backtraces into bare addresses. The image is not size-constrained.
#[test]
fn release_profile_keeps_symbols_for_profiling_and_backtraces() {
    assert!(
        !release_profile().contains("strip"),
        "callgrind attribution and panic backtraces both need the symbol table"
    );
}
