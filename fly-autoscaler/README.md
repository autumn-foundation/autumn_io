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

- `autumn-io` already deployed, with at least one machine (started or
  stopped). fly-autoscaler scales up by cloning an existing machine, so it
  cannot start from zero machine *objects*.
- The `fly` CLI, authenticated against the org `autumn-io` runs under.

## Why the first attempt never worked

The first version of this config queried autumn-web's
`autumn_http_requests_total`, which reached Fly's Prometheus only through a
`[metrics]` scrape of `/actuator/prometheus`. Fly's scraper addresses the
machine by its private IP, and the prod profile's `[security.trusted_hosts]`
rejects an IP-literal Host with `400 Invalid Host header`. Nothing was ever
stored, the query came back empty, and fly-autoscaler fails the whole
reconcile on an empty result:

```
metrics collection failed: collect metric ("reqrate"): empty prometheus result
```

The config now queries `fly_edge_http_responses_count`, which fly-proxy
publishes for every HTTP app with no scrape of the app, and appends
`or vector(0)` so an idle, scaled-to-zero app reads as 0 req/s instead of an
error. `tests/fly_deploy_config.rs` pins both halves, and pins the 400 itself
against the real prod middleware stack.

## One-time setup

```bash
# 1. Create the autoscaler app itself (in the same org as autumn-io).
fly apps create autumn-io-autoscaler

# 2. A deploy-scoped token, limited to autumn-io, so the autoscaler can only
#    ever create/destroy/start/stop machines on that one app.
fly secrets set -a autumn-io-autoscaler \
  FAS_API_TOKEN="$(fly tokens create deploy -a autumn-io)"

# 3. A read-only org token so it can query Fly's built-in Prometheus. The org
#    must match the one in fly.toml's FAS_PROMETHEUS_ADDRESS (`personal`).
fly secrets set -a autumn-io-autoscaler \
  FAS_PROMETHEUS_TOKEN="$(fly tokens create readonly personal)"

# 4. Deploy. --ha=false: this is a single-purpose reconciliation loop: one
#    instance is the point, not a resilience gap. Fly's own docs deploy it
#    this way.
fly deploy --ha=false -c fly-autoscaler/fly.toml
```

## Verifying it's doing something

First, check the query and the expression without scaling anything.
`fly-autoscaler eval` runs one metrics collection and prints the machine
count it would target:

```bash
fly ssh console -a autumn-io-autoscaler -C "fly-autoscaler eval"
# { "created": { "min": 1, "max": 1 }, ... }   <- idle: floor of 1
```

Then watch the loop:

```bash
fly logs -a autumn-io-autoscaler
```

Every reconciliation (every 15s by default) logs a `reconciling` line with the
current started/stopped counts and the target. `fly status -a autumn-io`
shows the actual machine count moving in response.

To check the query by hand against Fly's Prometheus:

```bash
curl -sG https://api.fly.io/prometheus/personal/api/v1/query \
  -H "Authorization: $(fly tokens create readonly personal)" \
  --data-urlencode 'query=sum(rate(fly_edge_http_responses_count{app="autumn-io"}[1m])) or vector(0)'
```

## Troubleshooting

| Log line | Cause | Fix |
|---|---|---|
| `empty prometheus result` | The query matched no series. | Keep `or vector(0)` on the query. Query a `fly_*` series, not an app-scraped one (see above). |
| `401` / `unauthorized` from Prometheus | `FAS_PROMETHEUS_TOKEN` is missing, or is for a different org than `FAS_PROMETHEUS_ADDRESS`. | Re-run step 3 with the same org slug. |
| `list machines: ...unauthorized` | `FAS_API_TOKEN` is missing or scoped to another app. | Re-run step 2. |
| `no machine available to clone for scale up` | `autumn-io` has no machines at all. | `fly deploy` autumn-io first; one stopped machine is enough. |
| `cannot create machine, skipping`, repeating | The clone failed (capacity, region, token scope). v0.3 retries until the reconcile times out. | Read the error after it; check `FAS_API_TOKEN` is a deploy token for autumn-io. |

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
