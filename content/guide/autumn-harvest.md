+++
title = "Autumn Harvest"
description = "Durable workflow orchestration for Autumn apps, released as autumn-harvest 0.11.0."
order = 1015
+++

# Autumn Harvest

Autumn Harvest 0.11.0 is the durable workflow engine for Autumn apps. Use it
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
- Inbound webhook triggers, Kafka and SQS broker connectors, worker routing by
  queue and capability, and a single-writer SQLite backend for edge and
  single-server deployments.
- Transactional workflow start, so a run is enqueued inside the caller's own
  database transaction and never escapes a rolled-back write.
- Durable per-execution workflow logs, business-day timers, cross-type
  continue-as-new, and DAG-node compensation.
- A management API, dashboard, CLI, dead-letter inspection and replay, queue and
  per-activity pause/resume, execution lineage and parked-awaitable
  diagnostics, audit trails, search attributes, and telemetry hooks.
- Safer production defaults: mutating management routes fail closed outside
  the `dev` profile, activities stop after a 10-minute start-to-close default,
  every run's history is capped, and retry, re-dispatch and cron fires are
  jittered.
- Liveness and readiness probes with a draining state, workers that release
  their claims on shutdown, automatic load shedding by backlog age, and
  optional per-client API rate limiting.
- `HarvestEmbedding`, a single entry point for running Harvest inside a plain
  Axum service with no autumn-web at all.
- Keyed entities: serialized handlers over durable state for each key.
- Durable routes: an Autumn route that hands its work to a keyed workflow, so a
  retry with the same `Idempotency-Key` gets the stored result.
- `autumn-harvest-agent`, an optional crate that runs an LLM agent loop as a
  workflow, with token and cost budgets and replay-as-evaluation.
- Agent profile overlays under ceilings, best-of-N draws with scorers, and a
  context reset for long agent runs.
- `HarvestPlugin` on SQLite, for an Autumn app built on `autumn-web/sqlite`,
  a SQLite fleet app included.
- Cost-aware deferral: a worker holds named activity types while the
  autumn-web `CostSignal` is high, and runs them when it falls.
- Process roles: a `web` replica serves the API, and `worker` replicas run
  the workflows. Operators override run defaults at runtime.

## What's New in 0.11.0

Harvest 0.11.0 moves to autumn-web 0.11. Bump every Harvest crate and
`autumn-web` in one change:

```toml
[dependencies]
autumn-harvest = { version = "0.11", features = ["db"] }
autumn-harvest-plugin = "0.11"
autumn-web = "0.11"
```

A leftover autumn-web 0.10 dependency builds two copies, and `HarvestPlugin`
does not match the app's `AppBuilder`. If you name them, bump `diesel` to 2.4,
`diesel-async` to 0.10 and `rusqlite` to 0.42 too. The minimum supported Rust
version stays at 1.97.

Two changes affect every app:

- Dates and times use `jiff`. `DateTime<Utc>` is now `jiff::Timestamp`, and
  `chrono::Duration` is now `jiff::SignedDuration`. `autumn_harvest::jiff`
  re-exports the crate, and `autumn_harvest::chrono` is gone. No stored data
  changes.
- The worker, the scheduler, the retention janitor and the batch executor run
  as Autumn background tasks. Autumn places each one by the process role, and
  a `[roles.<name>]` preset turns them on or off with the capabilities
  `harvest.worker`, `harvest.scheduler` and `harvest.maintenance`. Set
  `server.shutdown_timeout_secs` to at least twice the worker shutdown
  timeout.

New in this release:

- [Activity default overrides](harvest-activity-default-overrides.md). An
  operator changes the retry policy, the timeouts and the fleet-wide cap of an
  activity type at runtime, with no deploy.
- [Claim slots](harvest-claim-slots.md) cap the claim statements that run at
  once on one shard, across the fleet. A [batch claim](harvest-claim-batch.md)
  lets one claim statement take up to 8 rows.
- [Pre-deploy analyses](harvest-dag-and-retry-analyses.md) forecast DAG
  slack, retry envelopes, retry storms and schedule herds from a unit test or
  a CI check.
- [The SQLite backend](harvest-sqlite-backend.md) runs external activities,
  and [HarvestPlugin on SQLite](harvest-sqlite-plugin.md) hosts the durable
  agent.
- [Durable agents](harvest-durable-agents.md) gain inference strategies (vote,
  model cascade, beam search and the dataset sweep), an adoption gate and a
  lesson library for the refine loop, profile reload, approval notices,
  Anthropic and OpenAI-compatible models, and episode, memory and transcript
  stores.
- [Sandbox activities](harvest-sandbox-activities.md) can run in a private
  file system.

Some behaviors changed. A batch or webhook-delivery start now takes the
execution timeout of its type. A refine run waits for an adoption. Turn on a
new feature only after every process runs 0.11.0. Before you deploy, read
the
[0.11.0 upgrade guide](https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/upgrading/0.11.0.md)
and the
[`chrono` to `jiff` guide](https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/upgrading/chrono-to-jiff.md).
Each change has its own section and check.

## Autumn Integration

Autumn apps wire Harvest in through the plugin. The plugin owns the workflow
runtime lifecycle, mounts the management API, and keeps the web process and
worker pool in the same operational footprint. Outside the `dev` profile, the
management API's mutating routes answer `401` until you declare an auth
boundary for them, for example with `api_with_auth` in place of `api`. The
management API reference covers
[token scopes and the authorizer hook](https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/management-api.md#authorization-token-scopes-and-the-authorizer-hook).

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
                .worker(WorkerConfig::default())
                .api("/api/harvest"),
        )
        .run()
        .await;
}
```

## Guide and Reference

The Harvest guide is folded into this site — the thirteen chapters below the
Harvest heading in the sidebar walk from a bare project skeleton through
activities, durable timers, signals, child workflows, idempotency, reliability
knobs, DAG schedules, worker routing, operations, testing, inbound webhooks,
and Kafka/SQS broker connectors. Running on plain Axum instead of autumn-web?
Take the fork after chapter 2,
[The first workflow on plain Axum](harvest-standalone-axum.md).

- [Start the Harvest guide](harvest-project-skeleton.md)
- [Harvest repository](https://github.com/autumn-foundation/autumn-harvest)
- [autumn-harvest on crates.io](https://crates.io/crates/autumn-harvest)
- [autumn-harvest API docs](https://docs.rs/autumn-harvest)
- [Autumn billing integration example](https://github.com/autumn-foundation/autumn-harvest/tree/trunk-dev/examples/billing-autumn-web)
- [Standalone runner example](https://github.com/autumn-foundation/autumn-harvest/tree/trunk-dev/examples/standalone-runner)

## When to Reach for It

Use `#[scheduled]` or a normal background task when a job is simple, short, and
fine to retry from the top. Use Harvest when the work needs durable state,
operator visibility, external handoffs, replayable history, or multiple steps
that must be recovered precisely after a deploy or crash.
