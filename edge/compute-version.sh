#!/usr/bin/env bash
# Print the deployment version: a hash of everything that decides what the edge
# lane sends back.
#
# This is the colo cache key's version (see `edge/worker/src/cache.js`), and its
# whole job is to make a deployment's entries unreachable the moment the *served
# bytes* can differ. So it has to cover the whole response-producing bundle, not
# just the capsule:
#
#   the capsule       the guides are embedded in it, so it decides the body
#   worker/src/*.js   the shim stamps headers, filters them, and decides what
#                     may be cached at all
#   security-headers  the list of headers the shim restores on every response
#   wrangler.toml     the origin a fallthrough goes to, and the routes bound
#
# Hashing only the capsule was the first version of this, and it left a real
# hole: a deploy that changed `security-headers.json` — say, to restore a fifth
# security header — produces a byte-identical `.wasm`, so every warmed colo
# would have kept serving the previous headers from cache, with the new Worker
# logic never running. The headers are the case that matters most here, which is
# exactly why the version cannot be blind to them.
#
# Content is hashed, never paths: `sha256sum`'s output embeds the filename, and
# an absolute path differs between a laptop and CI, which would make every build
# look like a new deployment and throw away warm caches for nothing. Ordering is
# fixed and the sort is locale-pinned, so the same inputs give the same version
# anywhere.
#
# Usage: compute-version.sh <edge_dir> <staged_wasm>
set -euo pipefail

edge_dir="${1:?usage: compute-version.sh <edge_dir> <staged_wasm>}"
staged_wasm="${2:?usage: compute-version.sh <edge_dir> <staged_wasm>}"

{
  cat "$staged_wasm"
  cat "$edge_dir/security-headers.json"
  cat "$edge_dir/worker/wrangler.toml"
  # `build/` is deliberately outside this: it holds the staged wasm (hashed
  # above) and the generated version module itself, which cannot hash itself.
  find "$edge_dir/worker/src" -name '*.js' -type f | LC_ALL=C sort | while read -r file; do
    cat "$file"
  done
} | sha256sum | cut -c1-16
