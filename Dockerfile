FROM rust:1.88-bookworm AS builder

WORKDIR /app

ARG TAILWIND_VERSION=v4.1.4

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p target/autumn \
    && curl -fsSL \
    -o target/autumn/tailwindcss \
    "https://github.com/tailwindlabs/tailwindcss/releases/download/${TAILWIND_VERSION}/tailwindcss-linux-x64" \
    && chmod +x target/autumn/tailwindcss

COPY Cargo.toml Cargo.lock autumn.toml build.rs tailwind.config.js ./
COPY content ./content
COPY src ./src
COPY static ./static
COPY migrations ./migrations

# Serialize compilation so concurrent rustc processes cannot stack their peaks.
# This trades build time for lower peak memory and does not affect the runtime
# binary's optimization level.
#
# It is NOT sufficient on its own. Measured peak RSS per rustc process for this
# dependency tree, with this setting already in effect:
#
#   autumn-web     2446 MB
#   autumn-macros  2395 MB
#   diesel         1296 MB
#   (everything else under 600 MB)
#
# So a single crate needs ~2.5 GB and the builder must have that much plus
# overhead. When autumn-web 0.7.0 landed, the builder OOM-killed autumn-macros
# outright (`signal: 9, SIGKILL`) — one process over the limit, nothing to do
# with parallelism. autumn-macros roughly doubled across that upgrade
# (0.6.0: 2239 MB -> 0.7.0: 4745 MB at opt-level 3).
#
# Two things that look like fixes but are not, so nobody re-tries them:
#   * Lowering opt-level for the proc-macro crates. Cargo already builds them
#     via the `build-override` profile at opt-level 0 — that is why the failing
#     rustc invocation carries no `-C opt-level`. An override is a no-op.
#   * Raising codegen-units. No measurable effect (2395 vs 2396 MB); the memory
#     is in rustc's front end, not LLVM.
#
# The lever that works is builder RAM. Size it to 8 GB, not 4: autumn-web needs
# more than the crate that actually failed, and this tree's requirement has
# grown fast.
ENV CARGO_BUILD_JOBS=1

RUN cargo build --locked --release --bin autumn_io

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 autumn

WORKDIR /app

COPY --from=builder /app/target/release/autumn_io /usr/local/bin/autumn_io
COPY --from=builder /app/autumn.toml /app/autumn.toml
COPY --from=builder /app/content /app/content
COPY --from=builder /app/static /app/static
COPY --from=builder /app/migrations /app/migrations

ENV AUTUMN_PROFILE=prod
ENV CARGO_PKG_NAME=autumn_io
ENV CARGO_PKG_VERSION=0.1.0
EXPOSE 3000

USER autumn

CMD ["autumn_io"]
