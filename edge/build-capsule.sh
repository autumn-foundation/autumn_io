#!/usr/bin/env bash
# Build the edge capsule and stage it for wrangler.
#
# This is what `autumn build` does for the edge step, spelled out so CI and a
# laptop run the same command:
#
#     cargo build --target wasm32-wasip1 --release --bin edge-capsule
#
# The `--bin` matters. This crate has several other binaries — the origin
# server, the static-site exporter, the profiling harnesses — and every one of
# them names `autumn-web`, which does not compile for wasm and is not supposed
# to. Only `edge-capsule` is an edge artifact.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact="$repo_root/target/wasm32-wasip1/release/edge-capsule.wasm"
staged="$repo_root/edge/worker/build/edge-capsule.wasm"

if ! rustup target list --installed | grep -qx wasm32-wasip1; then
  echo "error: the wasm32-wasip1 target is not installed." >&2
  echo "       run: rustup target add wasm32-wasip1" >&2
  exit 1
fi

echo "==> cargo build --target wasm32-wasip1 --release --bin edge-capsule"
(cd "$repo_root" && cargo build --target wasm32-wasip1 --release --bin edge-capsule)

mkdir -p "$(dirname "$staged")"
cp "$artifact" "$staged"

size=$(wc -c < "$staged")
printf '==> edge capsule: %s (%s KB)\n' "$staged" "$((size / 1024))"
