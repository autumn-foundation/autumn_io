# A release profile, and CI that runs the whole suite

Two changes that arrived together because the first one needs the second: an
optimised `[profile.release]` is a build configuration nothing was verifying,
and the CI that existed ran a third of the tests.

## What CI was missing

`docs-harness.yml` ran `docs_site` and `mcp_docs_api` — 62 tests, deliberately
scoped in #30 to the two binaries that need no database. Nothing ran the other
three test binaries.

The cost of that gap was already on trunk. The five fixture tests in
`tests/syntax_highlighting_backend.rs` had never passed, from the moment they
were written:

* #21 made scrollable code blocks keyboard-focusable, which changed the
  rendered opening tag to `<pre tabindex="0">`.
* #22, one commit later, added the golden tests — written against `<pre>`, with
  a `code_block` helper that located the block with `html.find("<pre>")`.

Every one of them failed with `a rendered code block` on a corpus that was
rendering and highlighting perfectly. `cargo test` was red on trunk for five
commits and no automation noticed.

The tag was re-recorded and the helper now matches `<pre`, which is what it
should always have matched: it exists to isolate the *highlighter's* output,
and the template's attributes are not its business. Every scope run inside the
goldens was byte-identical before and after, which is the reason re-recording
was the right call here and is not the default response to a failure in that
file.

`ci.yml` replaces the harness workflow and runs `fmt`, `clippy
--all-targets -D warnings`, the full `cargo test --locked`, and a release
build. Nothing in the suite needs a service, so "fast, no infra" — the reason
#30 gave for its narrow scope — is satisfied by running all of it.

The release build is a separate job for a reason of its own. It compiles with
the Rust version parsed out of the `Dockerfile`'s `FROM rust:` line rather than
with stable, because those are far apart (1.88 versus 1.97 at the time of
writing) and a release that builds on stable but not on the pinned builder is a
deploy-time failure. Reading the version from the Dockerfile means the pin
cannot drift from CI.

## The release profile

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "unwind"
```

### How these were measured

Instruction counts, not wall clock. The first pass at this used timings and
they were actively misleading — this machine had unrelated containers running,
and on the search harness they disagreed with the instruction counts by nearly
30 percentage points. `Ir` is deterministic per binary, which is what makes a
1.7% difference reportable at all.

Everything below was taken in the `rust:1.88-bookworm` builder image on
aarch64-linux, through the harnesses in `src/bin/`, with every variant built at
`lto = "fat"` and `codegen-units = 1` so the only thing differing is the axis
named. Search is reported as a *marginal per-request* cost: run at 50 requests
per query, run again at 1, subtract, divide by the request delta. That removes
the one-time index build, which otherwise dominates and is identical across
variants. It is the method issue #23 used, and its build-only figure is quoted
alongside because that is the number the cold path pays.

The deploy target is x86_64 and these are aarch64. Instruction counts are
stable per binary but move with architecture and toolchain, as
`profile_docs_search.rs` already notes; the ranking is the durable part.

### opt-level: neither `s` nor `z`

| | search, marginal/request | cold-start render | index build | deploy binary |
|---|---|---|---|---|
| `opt-level = 3` | 2,171,562 | 3,395,498,611 | 3,705,079,502 | 26.32 MiB |
| `opt-level = "s"` | 2,135,722 (**-1.7%**) | 3,767,464,464 (**+11.0%**) | 4,097,506,597 (+10.6%) | 23.17 MiB |
| `opt-level = "z"` | 2,404,462 (**+10.7%**) | 4,302,839,402 (**+26.7%**) | 4,828,251,662 (+30.3%) | 24.53 MiB |

`"z"` is worth a second look in that table, because it fails on its own terms:
it is slower than `"s"` on every workload here *and* produces a larger binary
than `"s"` does. Whatever it is trading away, it is not buying size back. It is
strictly dominated, and the only real choice was between `3` and `"s"`.

The instinct that sent this to `"s"` was that search needs loop vectorisation,
and the direction is right — LLVM keeps the loop vectoriser on at `Os` and
turns it off at `Oz`, which is visible above as `"z"` losing 10.7% on the
search path where `"s"` loses nothing. But the marginal search path is not
where the size levels are paid for. The cold-start render is, and `"s"` gives
up 11% of it.

That path matters more here than its name suggests. `fly.toml` runs
`min_machines_running = 0` with `auto_stop_machines = 'stop'`, so every
scale-to-zero is followed by a request that pays the full render, and it is the
one a real visitor waits on. Spending 11% of it to save a couple of MiB of a
binary in an image nothing is measuring is the wrong trade, and `"z"` asks for
27%.

### lto = "fat", codegen-units = 1

Where the size win actually is: **36,580,752 -> 27,601,240 bytes, -24.5%**, on
a binary built at the same `opt-level = 3` either way. That is nearly four times
what dropping to `"s"` would save from here, and it costs nothing at runtime.

It is also the piece that needed a safety check rather than a benchmark.

The Dockerfile carries a long note about builder memory, written after
`autumn-macros` was OOM-killed outright, concluding that raising codegen units
had no measurable effect because the memory lives in rustc's front end. Fat LTO
is the one change that could plausibly invalidate that: it ends the build with a
single link holding the whole dependency graph's IR, a shape none of those
measurements covered.

It does not. With `CARGO_BUILD_JOBS=1`, peak RSS across the whole build is
**4401 MB**, against the 4409 MB that note records without LTO, and the LTO link
in isolation peaks at **3909 MB in 71 s** — below the cost of compiling
`autumn-web`. The ceiling is unchanged, still in the front end, and 8 GB still
covers it. The Dockerfile now says so, so the next person to read that essay
does not have to re-derive it.

### jemalloc: measured, and declined

Worth asking, because DHAT says this is an allocation-heavy workload: a
search run churns **2,788,980 blocks and 1.64 GiB** through the allocator while
holding a peak of only **23.1 MiB live in 30,463 blocks**, and the cold-start
render is the same shape (2,698,164 blocks, 1.57 GiB total, 20.7 MiB peak
live). High churn against a small live set is exactly where a better allocator
earns its keep.

It does earn something, consistently, and not very much:

| | glibc malloc | jemalloc |
|---|---|---|
| search, marginal/request (default) | 2,171,562 | 2,161,974 (**-0.4%**) |
| search, marginal/request (typeahead) | 1,554,921 | 1,543,302 (**-0.7%**) |
| cold-start render | 3,395,498,611 | 3,333,092,559 (**-1.8%**) |
| search index build | 3,705,079,502 | 3,643,035,494 (-1.7%) |

Against that, it costs about 490 KiB of binary, adds a second bundled C library
to a builder image whose C toolchain story is already load-bearing and
carefully documented, and raised peak RSS by 6-10 MiB in the harnesses. The
last one is affordable on a 256 MB machine — the live set is 23 MiB — but it is
a real cost for a 1-2% return.

The deciding argument is what jemalloc is *for*. Its headline advantage is
per-thread arenas removing contention between allocating threads, and
`fly.toml` runs `cpus = 1`. The configuration that would pay for it is not the
configuration this app is deployed in.

So: not now, and this is a one-line change if the machine ever grows CPUs or
the allocation profile shifts. DHAT cannot compare the two allocators directly,
for the record — it intercepts libc `malloc`/`free`, and `tikv-jemallocator`
exports prefixed symbols and goes to `mmap`, so it is invisible to DHAT. The
comparison above is callgrind's, which counts the instructions inside either
allocator without caring which one it is.


## Three things that look like wins here and are not

**`panic = "abort"`.** It is the standard companion to this profile — smaller
binary, no landing pads — and it is wrong for this app specifically. Autumn
uses unwinding as a correctness mechanism, not just for diagnostics:
`autumn-web`'s `db.rs` catches panics inside transactions because "a panic that
unwound without a rollback would leave deadpool free to recycle this connection
with an open, uncommitted write transaction", and the job runner and event
dispatcher each isolate a panicking handler from its siblings the same way.
Under `panic = "abort"` every one of those isolation boundaries becomes a
process kill. The profile sets `panic = "unwind"` explicitly so that the choice
reads as deliberate.

**`strip = "symbols"`.** The largest remaining size lever, and it costs two
things this repo actively uses. `src/bin/profile_docs_search.rs` already warns
about it in prose — "Attribution needs symbols, so do not add `strip` to
`[profile.release]` without expecting hex addresses here" — because the
callgrind workflow that produced the numbers in the last two plan documents
attributes by symbol. It also turns production panic backtraces into
addresses. The image is not size-constrained, so the symbols stay.
`strip = "debuginfo"` is not a middle ground: release builds carry no debuginfo
to begin with, so it removes nothing.

**`-C target-cpu`.** The tempting one for a search workload, and close to
inert here. The hot paths that would benefit — `aho-corasick` and `memchr` —
already select their SIMD implementation at runtime, so raising the baseline
does not change what they execute. What it would add is a way for the app to
die with `SIGILL` on a Fly host older than the chosen level, in exchange for a
gain nothing here has measured.

## What is still on the table

Profile-guided optimisation is the one unexplored lever with a plausible
double-digit win, and this repo is unusually well set up for it: PGO needs a
representative, deterministic training workload, and `profile_docs_render` and
`profile_docs_search` are exactly that, already written and already the
workloads whose instruction counts the last two plan documents optimised. It
would mean a three-stage Docker build — instrument, train, rebuild — which is
why it is written down here rather than done.
