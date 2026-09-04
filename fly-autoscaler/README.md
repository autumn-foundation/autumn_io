# autumn-io-autoscaler

Companion Fly app that scales `autumn-io`'s machine count by a Prometheus
metric ([Fly's autoscale-by-metric feature](https://fly.io/docs/launch/autoscale-by-metric/)),
using Fly's official [`flyio/fly-autoscaler`](https://github.com/superfly/fly-autoscaler)
image. Config lives in [`fly.toml`](./fly.toml); this file is the setup
walkthrough, run by hand — nothing here is automated by CI.

It grows/shrinks how many `autumn-io` machine *objects* exist so the proxy
has more to autostart into during a real traffic surge. It does not take over
start/stop of those machines — `../fly.toml`'s `auto_stop_machines` /
`auto_start_machines` / `min_machines_running = 0` still do that, so
idle-to-zero is unaffected. See the comment header in `fly.toml` for why the
split matters.

## Prerequisites

- `autumn-io` already deployed (its `fly.toml` has the `[metrics]` block
  pointing Fly's platform scraper at `/actuator/prometheus` — that's what
  this autoscaler queries).
- The `fly` CLI, authenticated against the org `autumn-io` runs under.

## One-time setup

```bash
# 1. Create the autoscaler app itself (in the same org as autumn-io).
fly apps create autumn-io-autoscaler

# 2. A deploy-scoped token, limited to autumn-io, so the autoscaler can only
#    ever create/destroy/start/stop machines on that one app.
fly secrets set -a autumn-io-autoscaler \
  FAS_API_TOKEN="$(fly tokens create deploy -a autumn-io)"

# 3. A read-only org token so it can query Fly's built-in Prometheus.
#    Replace <ORG_SLUG> with `fly orgs list`'s output — the same value that
#    belongs in fly.toml's FAS_PROMETHEUS_ADDRESS.
fly secrets set -a autumn-io-autoscaler \
  FAS_PROMETHEUS_TOKEN="$(fly tokens create readonly <ORG_SLUG>)"

# 4. Fill in <ORG_SLUG> in fly.toml's FAS_PROMETHEUS_ADDRESS, then deploy.
#    --ha=false: this is a single-purpose reconciliation loop: one instance
#    is the point, not a resilience gap. Fly's own docs deploy it this way.
fly deploy --ha=false -c fly-autoscaler/fly.toml
```

## Verifying it's doing something

```bash
fly logs -a autumn-io-autoscaler
```

Every reconciliation (every 15s by default) logs the query result and the
machine count it computed. `fly status -a autumn-io` shows the actual machine
count moving in response.

## Tuning

`FAS_PROMETHEUS_QUERY` and `FAS_CREATED_MACHINE_COUNT` in `fly.toml` are
starting points — a 15 req/s-per-machine budget and a 1–3 machine range —
chosen without production traffic data. After this has run for a while,
revisit both against real numbers from `fly metrics` or the Fly dashboard.

## Rolling this back

```bash
fly apps destroy autumn-io-autoscaler
```

`autumn-io` itself is untouched — this only removes the reconciliation loop,
not any machines it created. Existing `autumn-io` machines keep running;
`min_machines_running = 0` on `autumn-io` continues to scale them to zero on
idle exactly as before this was set up.
