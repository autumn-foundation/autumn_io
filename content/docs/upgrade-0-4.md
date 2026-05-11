---
title: Upgrade to 0.4.0
description: Prepare an existing Autumn app for the 0.4.0 release.
order: 60
---

# Upgrade to 0.4.0

Autumn 0.4.0 is the release this site is being built for. Treat this page as the launch upgrade checklist for existing apps.

## Before upgrading

Create a branch and make sure the current app is green:

```bash
git switch -c upgrade/autumn-0-4
cargo fmt --all
cargo test
```

Fix existing failures before changing the framework dependency. Otherwise the upgrade becomes archaeology with a compiler.

## Update the dependency

When 0.4.0 is published, update `Cargo.toml`:

```toml
[dependencies]
autumn-web = "0.4"
```

Then refresh the lockfile:

```bash
cargo update -p autumn-web
```

If you are testing a release candidate, use the exact Git revision or local path specified by the release notes.

## Review configuration

Compare your `autumn.toml` with the 0.4.0 defaults:

```toml
[server]
host = "127.0.0.1"
port = 3000

[health]
path = "/health"
```

Pay attention to health paths, session backend settings, telemetry settings, and any deployment-specific overrides.

## Re-test public routes

Run route and integration tests after the dependency update:

```bash
cargo test
```

Then smoke test the app:

```bash
cargo run
curl -f http://127.0.0.1:3000/
curl -f http://127.0.0.1:3000/health
```

If your app serves static assets, check at least one CSS, JavaScript, and image path.

## Document app-specific changes

If your app needed code changes for 0.4.0, record them near the app docs or release notes. Future you will not remember which clever workaround was intentional. Future you is a liar with a calendar.
