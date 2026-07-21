+++
title = "Autumn Harvest"
description = "Durable workflow orchestration for Autumn apps, released as autumn-harvest 0.3.0."
order = 25
+++

# Autumn Harvest

Autumn Harvest 0.3.0 is the durable workflow engine for Autumn apps. Use it
when a request should start work that must survive restarts, retries, timers,
signals, child workflows, and scheduled dependency graphs.

It is the Autumn answer to the work that does not fit inside a single HTTP
request: onboarding flows, billing retries, multi-step imports, approval
handoffs, periodic reconciliation, and other orchestration that needs a real
history instead of a hopeful background task.

## What Harvest Adds

- Event-sourced workflow execution backed by Postgres.
- `#[workflow]`, `#[activity]`, and `#[dag]` macros for Rust-native workflow
  definitions.
- Activities with retries, timeouts, heartbeats, idempotency keys, queues, and
  concurrency caps.
- Durable timers, signals, queries, child workflows, cancellation, workflow
  reset, and continue-as-new support.
- DAG schedules, manual triggers, offline linting, simulation, and profiling.
- A management API, dashboard, CLI, dead-letter inspection and replay, audit
  trails, search attributes, and telemetry hooks.

## Autumn Integration

Autumn apps wire Harvest in through the plugin. The plugin owns the workflow
runtime lifecycle, mounts the management API, and keeps the web process and
worker pool in the same operational footprint.

```rust
use autumn_harvest::prelude::*;
use autumn_harvest_plugin::HarvestPlugin;
use autumn_web::prelude::*;

#[workflow]
async fn onboarding(ctx: &WorkflowContext, user_id: i64) -> HarvestResult<()> {
    ctx.execute_activity_raw(
        "send_welcome_email",
        serde_json::json!({ "user_id": user_id }),
        "default",
    )
    .await?;

    ctx.timer("follow-up-delay", 30).await?;
    Ok(())
}

#[activity(start_to_close = "30s")]
async fn send_welcome_email(
    _ctx: &ActivityContext,
    input: serde_json::Value,
) -> HarvestResult<serde_json::Value> {
    Ok(serde_json::json!({ "sent": input["user_id"].is_i64() }))
}

#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .plugin(
            HarvestPlugin::new()
                .workflows(workflows![onboarding])
                .activities(activities![send_welcome_email])
                .api("/api/harvest"),
        )
        .run()
        .await;
}
```

## Guide and Reference

- [Harvest getting started guide](https://github.com/autumn-foundation/autumn-harvest/tree/trunk/docs/getting-started)
- [Harvest repository](https://github.com/autumn-foundation/autumn-harvest)
- [autumn-harvest on crates.io](https://crates.io/crates/autumn-harvest)
- [autumn-harvest API docs](https://docs.rs/autumn-harvest)
- [Autumn billing integration example](https://github.com/autumn-foundation/autumn-harvest/tree/trunk/examples/billing-autumn-web)
- [Standalone runner example](https://github.com/autumn-foundation/autumn-harvest/tree/trunk/examples/standalone-runner)

## When to Reach for It

Use `#[scheduled]` or a normal background task when a job is simple, short, and
fine to retry from the top. Use Harvest when the work needs durable state,
operator visibility, external handoffs, replayable history, or multiple steps
that must be recovered precisely after a deploy or crash.
