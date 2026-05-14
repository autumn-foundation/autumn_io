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
