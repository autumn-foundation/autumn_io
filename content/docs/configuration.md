---
title: Configuration
description: Configure server, logging, health, database, and session settings.
order: 30
---

# Configuration

Autumn runs with useful defaults, then lets you override behavior through `autumn.toml` and environment-specific profiles.

## Server

The default development server listens on `127.0.0.1:3000`:

```toml
[server]
host = "127.0.0.1"
port = 3000
```

Use a public bind address only when your deployment environment expects the app to listen on all interfaces:

```toml
[server]
host = "0.0.0.0"
port = 3000
```

## Logging

Set the log level in configuration:

```toml
[log]
level = "info"
```

Use `debug` while diagnosing local behavior. Use `info` or a deployment-specific value in production unless you have a reason to turn the volume up.

## Health checks

The default health endpoint is `/health`:

```toml
[health]
path = "/health"
```

Deployment platforms can point liveness and readiness probes at this path. Keep health checks cheap and deterministic.

## Database

When your app uses a database, configure the connection pool explicitly:

```toml
[database]
url = "postgres://user:pass@localhost:5432/autumn_app"
pool_size = 10
connect_timeout_secs = 5
```

Prefer environment-specific secrets for production credentials. Do not commit real database passwords to `autumn.toml`.

## Sessions

For production session storage, externalize sessions to Redis:

```toml
[session]
backend = "redis"

[session.redis]
url = "redis://redis:6379"
key_prefix = "autumn:sessions"
```

Local development can use simpler defaults. Production should use a shared store so restarts and multiple replicas do not drop user state.
