+++
title = "Autumn Harvest"
description = "Durable workflow orchestration for Autumn apps, released as autumn-harvest 0.9.0."
order = 1015
+++

# Autumn Harvest

Autumn Harvest 0.9.0 is the durable workflow engine for Autumn apps. Use it
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

## What's New in 0.9.0

Harvest 0.9.0 stays on autumn-web 0.9. Bump every Harvest crate in one
change:

```toml
[dependencies]
autumn-harvest = { version = "0.9", features = ["db"] }
autumn-harvest-plugin = "0.9"
autumn-web = "0.9"
```

The minimum supported Rust version rises from 1.88 to 1.97.

New in this release:

- [HarvestPlugin on SQLite](harvest-sqlite-plugin.md). The `sqlite` feature of
  `autumn-harvest-plugin` boots the plugin in an Autumn app on the SQLite
  lane. Harvest keeps its own file next to the app database. A fleet app can
  give each tenant its own Harvest file, or point Harvest at Postgres in
  split mode.
- [Cost-aware deferral](harvest-cost-deferral.md). A worker holds named
  activity types while the autumn-web `CostSignal` is high. A held task stays
  `PENDING`, uses no attempt, and holds no worker slot. One threshold drives
  Autumn jobs and Harvest activities.
- [Agent profile overlays](harvest-durable-agents.md). An overlay changes an
  agent's system prompt, guardrails, model and bounds under fixed ceilings.
  A versioned store keeps each version, and a rollback activates an earlier
  one. Each op records the overlay it read, so replay never reads the store.
- Best-of-N draws and scorers. `run_draws` runs each draw as a child run, a
  `Scorer` records a verdict, and `select` picks the best draw.
- Context reset. Near the window cap, a long agent run saves progress notes
  and starts again from the prompt, the task and the notes.
- New indexes for signal reads and dead-letter reads, and a retention index
  migration that no longer deadlocks.

Some APIs and behaviors changed. The `schema` feature moves to `schemars`
1.x, three Diesel tables split into a core and an extension declaration, and
`reqwest` now uses the OS trust store. Turn on the new agent features only
after every worker runs 0.9.0. Before you deploy, read the
[0.9.0 upgrade guide](https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/upgrading/0.9.0.md).
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
