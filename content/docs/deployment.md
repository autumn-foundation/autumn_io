---
title: Deployment
description: Build, configure, containerize, and smoke test an Autumn app.
order: 50
---

# Deployment

Autumn apps deploy like normal Rust binaries: build a release artifact, provide configuration, expose a port, and wire health checks.

## Release build

Build an optimized binary:

```bash
cargo build --release
```

Run the release binary with the production profile:

```bash
AUTUMN_PROFILE=prod ./target/release/autumn_io
```

On Windows PowerShell:

```powershell
$env:AUTUMN_PROFILE = "prod"
.\target\release\autumn_io.exe
```

## Docker

Use a multi-stage Docker build so the runtime image only contains the app and runtime assets:

```dockerfile
FROM rust:1.86-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/autumn_io /usr/local/bin/autumn_io
COPY --from=builder /app/autumn.toml /app/autumn.toml
COPY --from=builder /app/static /app/static
ENV AUTUMN_PROFILE=prod
EXPOSE 3000
CMD ["autumn_io"]
```

The project Dockerfile also installs Tailwind during the builder stage so generated CSS exists in the runtime image.

## Health checks

After deployment, check the health endpoint before sending traffic:

```bash
curl -f http://127.0.0.1:3000/health
```

Use the same path for platform probes unless you intentionally configure separate live and ready endpoints.

## Configuration

Keep deployment-specific values out of source control:

```toml
[server]
host = "0.0.0.0"
port = 3000

[log]
level = "info"
```

Use environment variables or your platform's secret store for database URLs, Redis URLs, and credentials.

## Smoke test

Before calling a deployment good, hit the public routes:

```bash
curl -f http://127.0.0.1:3000/
curl -f http://127.0.0.1:3000/docs/quickstart
curl -f http://127.0.0.1:3000/static/css/site.css
```

This catches missing content and static asset copy mistakes before users do.
