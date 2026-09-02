# Syntect Regex Backend: `default-fancy` vs `default-onig`

Date: 2026-09-02
Status: Accepted — measured, implemented
Issue: https://github.com/autumn-foundation/autumn_io/issues/19

## Goal

Decide, with evidence, whether `syntect` should keep the pure-Rust
`default-fancy` regex backend or move to syntect's own upstream default,
`default-onig` (Oniguruma, a C matcher), for the cold-start docs render.

Issue #19 profiled `autumn_io::site_docs()` — the one-time render of the 140
embedded guides that both `build_site` and the first request after a
scale-to-zero cold boot pay (`content/guide` holds 141 Markdown files;
`docs-smoke` is a release-rehearsal checklist deliberately kept off the site,
and issue #19 counted files rather than rendered pages) — and found:

- 76.81% of 13.75 B instructions are in the regex engine
  (`fancy_regex` + `regex_automata` + `regex_syntax`), 42.24% in
  `fancy_regex::vm::run` alone (the backtracking VM).
- 71.5% of 5.13 M allocation blocks and 67.5% of 1.21 GB allocated bytes are
  the same backtracking VM saving capture state per match attempt.
- `autumn_io`'s own code is 0.15% of instructions and ~0% of allocations.

The issue deliberately stopped short of a PR: the lever is a build/dependency
decision, and this repo has been burned twice by Fly build OOM (#9, #16).
This document is the human call the issue asked for, made against measurements
rather than assertion.

## Non-goals

- Rewriting `src/docs.rs`. The profile says there is nothing there to win.
- Changing rendered HTML. A backend swap that changes highlighting output is a
  regression, not an optimisation.
- Changing the runtime image's memory budget (256 MB) or the builder's.

## Brainstorming — candidate levers

Everything considered, before narrowing:

1. **Swap to `default-onig`.** Issue #19's hypothesis. Moves TextMate-grammar
   matching from a pure-Rust backtracking VM to Oniguruma's C matcher.
2. **Stay on `default-fancy`, trim the syntax set.** Load only the grammars the
   guides actually use instead of `load_defaults_newlines()`.
3. **Precompute highlighted HTML at build time.** Render the corpus in `build.rs`
   or a committed generated artifact; runtime cold start pays ~zero.
4. **Render guides lazily, per request.** Cold boot renders one page, not 141.
5. **Parallelise the render** (rayon over pages).
6. **Bake a rendered-HTML cache into the Docker image** at build time.
7. **Drop server-side highlighting** for a client-side highlighter.
8. **Warm the registry in a startup background task** so the first request is
   not the one that pays.
9. **Cap highlighting** at N lines or to a language allow-list.
10. **Bump `fancy-regex`** and take upstream's own perf work.

### Narrowing

- (5) is dead on arrival: the Fly VM is `cpus = 1`, `cpu_kind = 'shared'`.
- (7) regresses no-JS rendering and SEO, and moves cost onto every reader
  instead of paying it once.
- (8) does not remove work; it hides it, and on a 256 MB machine the
  allocation churn is itself a risk.
- (9) silently degrades the docs.
- (2) and (10) are marginal: syntect compiles grammar regexes lazily on first
  use, so unused grammars are already close to free, and the cost is inside the
  grammars we do use.
- (3), (4) and (6) are real, larger alternatives — they attack *when* the work
  happens rather than how fast it is. They are strictly bigger changes than a
  feature flag, and they are still available if (1) fails. Recorded here as the
  fallback path, not attempted in this change.
- (1) is the smallest change with the largest measured lever, and it is the
  option the issue asked a human to rule in or out. Evaluate it first.

## Reverse brainstorming — how would this change hurt us?

Asking "how do we make this fail?" turns directly into the test list. The
Guard column distinguishes a *standing test* (fails in CI or `cargo test`
forever after) from something *verified once* during this evaluation, because
the two are not the same protection.

| Failure mode | How it bites | Guard |
|---|---|---|
| `onig_sys` built with its default `generate` feature | `bindgen` needs libclang, which the builder has not got; fails in Docker only | Standing test asserting the locked `onig_sys` deps stay `cc` + `pkg-config` |
| Builder image lacks a C toolchain | `onig_sys` fails to compile in Docker but works locally | Real `docker build` of the actual Dockerfile |
| Fly builder OOM, third time (#9, #16) | Deploy dies on `SIGKILL`, same class of failure as before | Measure peak builder RSS under `CARGO_BUILD_JOBS=1`; compare against the documented 8 GB sizing |
| Rendered HTML drifts between engines | Silent visual regression across 140 guides | Full-corpus byte-for-byte HTML diff, both backends (verified once), plus committed golden output for representative blocks (standing test) |
| Runtime image misses a shared library | Binary builds, container will not start | Run the built binary in the `debian:bookworm-slim` runtime stage |
| Someone reverts the flag later, unaware | Regression back to the backtracking VM | Standing test pinning the feature, with the rationale in the failure message |
| A grammar regex the engine rejects | syntect's lazy compile `expect`s, so it panics rather than erroring; the panic poisons the `SITE_DOCS` `LazyLock` and every later docs request panics too | Not newly introduced — the same hazard existed on `fancy-regex`, and the direction of travel is favourable, since the bundled grammars are written for Oniguruma. Bounded, not closed: compilation is lazy, so the byte-identical corpus render certifies only the patterns those 140 pages reach |
| Build time balloons | Slower deploys | Time the builds, report both |
| "Faster" claim rests on wall clock | Noise on shared CPUs is not evidence | Instruction counts (callgrind) and allocation counts (dhat), same harness as #19 |
| Harness is not reproducible | Nobody can re-check the claim | Commit `src/bin/profile_docs_render.rs` |

## Six thinking hats

**White (facts).** `default-fancy` is 76.81% regex instructions. Syntect's own
upstream default is `default-onig`. This repo has only ever used
`default-fancy`, set in `9082954` when highlighting was added. Runtime is
256 MB; the builder is sized at 8 GB with `CARGO_BUILD_JOBS=1`. The builder
image is `rust:1.88-bookworm`, which is `buildpack-deps`-based and ships gcc.

**Red (instinct).** Pulling a C dependency into a clean pure-Rust build feels
like a step backwards, and this repo's OOM history makes any build-graph change
feel expensive. Against that: 820 MB of allocation churn on a 256 MB machine is
uncomfortable to leave in place once measured.

**Black (risks).** Build OOM; C toolchain assumptions on contributors' machines;
`onig_sys` is a thinner-maintained C binding and a memory-safety surface the
pure-Rust path does not have; `--locked` deploy failure if the lockfile lags;
output drift between two regex dialects.

**Yellow (upside).** The lever is the dominant cost, not a slice of it. Both
`build_site` and every cold boot benefit. It is syntect's own default, so it is
the better-tested path upstream. Lower allocation churn matters on the smallest
Fly machine. And it is one line to revert.

**Green (alternatives).** Make the decision cheap to revisit: commit the
harness, pin the choice in a test that explains itself, and record before/after
numbers in the PR. If onig fails on build memory, fall back to precomputing the
render at build time (option 3), which removes the cost rather than shrinking it.

**Blue (process).** Red/green/refactor. Red: tests that fail on `default-fancy`
and encode every failure mode above. Green: the flag, the lockfile, the
Dockerfile note. Refactor: tidy without moving the numbers. Then measure both
backends with the #19 harness and put the table in the PR.

## TDD plan

**Red.** `tests/syntax_highlighting_backend.rs`:

1. `syntect_uses_the_oniguruma_regex_backend` — `Cargo.toml` selects
   `default-onig`, not `default-fancy`.
2. `the_oniguruma_build_stays_free_of_bindgen` — the resolved `onig_sys` entry
   in `Cargo.lock` depends on `cc` and `pkg-config` only. Its own default
   feature would pull in `bindgen`, and with it a libclang requirement the
   builder image cannot meet; the chain that keeps it off is three crates deep
   and invisible from our manifest.
3. `dockerfile_builder_keeps_the_c_toolchain_oniguruma_needs` — the builder
   stage keeps, documents, and pins the C toolchain the backend now needs.
4. Golden output for representative code blocks (Rust, shell, TOML,
   unknown-language fallback, HTML metacharacters). Pinned as exact strings,
   not "contains a coloured span": syntect wraps *plain text* in a coloured
   span too, so the loose assertion passes even when highlighting has collapsed
   entirely — which is precisely the failure a backend swap could cause.

The first three fail on `default-fancy`. The goldens are recorded from it and
must survive the swap unchanged, which is the actual equivalence claim.

**Green.** Flip the feature, regenerate `Cargo.lock`, document and pin the
Dockerfile's build requirements. Nothing else.

**Refactor.** Only comment/structure cleanups; re-run the suite.

## Evidence gathered

Instruments are named per row, because they are not interchangeable. All
numbers come from one sandbox host (4 cores, 15.7 GB); the caveats section
says what that does and does not transfer to Fly.

The `default-fancy` baseline reproduces issue #19 to within 0.002% of
instructions (13,750,293,934 against its 13,750,553,365) and exactly on
allocations (5,128,020 blocks / 1,214,627,729 bytes), so the two sides are
measuring the same thing.

### Cold-start render (`autumn_io::site_docs()`, 140 pages, 2.31 MB Markdown)

`profile_docs_render`, the harness from issue #19, now committed.

| Metric | Instrument | `default-fancy` | `default-onig` | Change |
|---|---|---:|---:|---|
| Instructions | callgrind `Ir` | 13,750,293,934 | 3,606,291,799 | **−73.8%** (3.81x) |
| Wall clock, median of 5 | `time.perf_counter` around the process | 1.760 s | 0.533 s | **−69.7%** (3.30x) |
| Process peak RSS | `getrusage(RUSAGE_CHILDREN).ru_maxrss` | 101.8 MB | 25.5 MB | **−75.0%** |
| Peak live heap | dhat, sum of `gb` at t-gmax | 92.80 MB | 19.46 MB | **−79.0%** |
| Allocation blocks | dhat, sum of `tbk` | 5,128,020 | 2,697,855 | −47.4% |
| Bytes allocated (churn) | dhat, sum of `tb` | 1.21 GB | 1.63 GB | **+34.5%** |

Instructions fall further than time does — 3.81x against 3.30x — because
Oniguruma is `-O2` C and `fancy-regex` is Rust, and their instructions do not
cost the same. **The honest speed figure is ~3.2-3.3x; 3.81x is the instruction
ratio and is not a speedup claim.**

The one metric that moves the wrong way is total bytes allocated. Oniguruma
takes large transient match buffers (`match_at` alone accounts for 702 MB
across 136,398 blocks) and frees them immediately, where `fancy-regex` took
many small capture-state allocations and held far more of them live.

Two halves of that objection, answered separately:

- *CPU cost of the churn* — answered by the instruction count, which already
  includes the whole `malloc`/`free` family (7.01% of the baseline in issue
  #19's own breakdown). More bytes through fewer, larger blocks costs fewer
  instructions in total.
- *Resident memory under sustained traffic* — the dhat and single-render
  numbers cannot speak to it, so it was measured directly instead; see the
  runtime table below. It does not materialise.

The profile's shape changes accordingly. `fancy_regex::vm::run` at 42.24% is
gone; the top frame becomes Oniguruma's `match_at` at 18.10%, and syntect's own
`ParseState::parse_line` rises to 9.07% of a much smaller total. Regex work is
still the largest single bucket — it is a regex-driven highlighter — but it is
no longer three quarters of the run.

### Rendered output

`build_site` was run on both backends and the exports compared:

- All **141 exported HTML pages (9.1 MB) — 140 guides plus the index — are
  byte-for-byte identical**.
- `manifest.json` matches once `generated_at` is excluded; the export already
  writes a timestamp and serialises routes from a `HashMap`, so its byte order
  varies run to run on either backend. Unrelated to this change.

That diff was a one-time check, so `tests/syntax_highlighting_backend.rs` also
pins the exact highlighted markup of representative Rust, shell, TOML and
unknown-language blocks. Those goldens are the standing guard: the next
`syntect` or `onig` bump that moves highlighting fails loudly instead of
quietly restyling 140 guides.

### Build cost (real `Dockerfile`, `--no-cache`, `CARGO_BUILD_JOBS=1`, one host)

| Metric | `default-fancy` | `default-onig` | repeat of `default-onig` |
|---|---:|---:|---:|
| Wall clock | 15m27.5s | 15m19.9s | 15m19.4s |
| Peak `rustc` RSS | 4409 MB | 4408 MB | 4412 MB |
| Peak RSS across all build processes | 5312 MB | 5350 MB | 5347 MB |
| Largest `cc1` translation unit | sqlite3.c, 354 MB | sqlite3.c, 353 MB | sqlite3.c, 353 MB |
| Largest Oniguruma `cc1` unit | — | regparse.c, 73 MB | regparse.c, 78 MB |
| Runtime binary | 37,513,936 B | 36,982,440 B | — |
| Runtime image on disk | 184 MB | 183 MB | — |

The OOM risk this repository has twice been bitten by does not materialise,
and the mechanism says why before the numbers do: the ceiling is `rustc` on
`autumn-macros`/`autumn-web` (2446/4745 MB per the Dockerfile's own note),
which this change does not touch, and `onig` *replaces* `fancy-regex` +
`bit-set` + `bit-vec` in the graph rather than adding to it. The measurements
are consistent with that — peak `rustc` within 4 MB, peak across all processes
+38 MB (0.7%), build time 8 seconds *shorter*, and the repeat build landing
within 0.5 s of its twin, which is the only estimate of run-to-run spread on
offer.

The 353 MB `cc1` peak is sqlite3.c, on both sides. Oniguruma's own largest
translation unit is 73-78 MB. An earlier draft of this document attributed the
353 MB to `onig_sys`; that was wrong, and the identical figure in the "before"
column should have given it away.

### Runtime, under the deployed machine's shape

`docker run --memory=256m --memory-swap=256m --cpus=1`, prod profile, memory
read with `docker stats` (`memory.current` less inactive file pages).

| Metric | `default-fancy` | `default-onig` |
|---|---:|---:|
| Idle after boot | 4.01 MiB | 3.76 MiB |
| First `/docs/{slug}` request (renders the corpus) | 1.691 s | 0.527 s |
| Warm request, median of 5 | 0.0018 s | 0.0012 s |
| RSS after that first request | 95.93 MiB | 21.95 MiB |
| RSS after a further 1,000 requests across 20 guides | 95.88 MiB | 22.18 MiB |

Two things this settles. The cold-start win survives the real constraint —
3.21x on one CPU and 256 MB, in line with the 3.30x harness figure. And the
+34.5% allocation churn does not accumulate: after a thousand requests the
resident set has moved by 0.23 MiB, well inside noise, on a machine that would
have OOM-killed the container had glibc's arenas grown. Neither container
logged an OOM or a panic.

On the 256 MB machine that is 37% of the budget before and 9% after.

`ldd` on the runtime binary shows no new shared object — Oniguruma is
statically linked — so the `debian:bookworm-slim` stage needs nothing added.
The Dockerfile now sets `RUSTONIG_STATIC_LIBONIG=1` to pin that rather than
rely on no system libonig ever being installed in the builder.

## Decision

**Adopt `default-onig`.**

Every risk the reverse-brainstorm raised was measured rather than argued:
build memory is unchanged, the builder image already carries the C toolchain,
the lockfile resolves `--locked` inside Docker, the runtime image needs no new
library, the rendered corpus is byte-identical, and resident memory is flat
under sustained traffic on a 256 MB machine. The cost is a C dependency in the
build graph; the return is a 3.2x faster cold start and a quarter of the
resident memory on the smallest machine we deploy to.

The fallback (option 3, precomputing the render at build time) stays on the
table as a later, larger change if the cold start ever needs to go to zero.
It is not needed to close issue #19.

## Caveats on the evidence

- Every number was taken on this sandbox host, not on Fly. The runtime rows
  were taken under `--memory=256m --cpus=1`, which matches the machine's
  shape but not its neighbours: a shared-cpu-1x under contention will be
  slower in absolute terms. The *ratios* are CPU-bound and should carry.
- The build rows were not taken on Fly's builders at all. What transfers is
  the mechanism — the ceiling is rustc on crates this change does not touch —
  not the absolute megabytes. The first deploy is still the real test of the
  OOM question, and this document does not claim to have closed it remotely.
- The sandbox intercepts TLS, so the two base images were re-tagged locally
  with its CA added. Every instruction in the `Dockerfile` itself ran verbatim.
- Build wall clock is n=1 per side with one repeat on the "after" side. The
  repeat agreeing to 0.5 s is the basis for treating the 8-second difference as
  noise; it is not a variance estimate on the "before" side.
- Instruction counts are stable per binary but not across toolchains. This repo
  pins no `rust-toolchain.toml`; the figures here are rustc 1.94.1 on
  x86_64-linux, and function-level attribution depends on the release profile
  not stripping symbols (there is no `[profile.release]` section today).
