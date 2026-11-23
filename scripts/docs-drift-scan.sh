#!/usr/bin/env bash
# Folio drift scan: every `autumn_web::...` or `autumn_harvest*::...` path in
# content/ whose final segment is not defined/re-exported anywhere in the pinned
# crate sources (autumn-web, autumn-macros, autumn-harvest{,-agent,-plugin,-sqlite}).
# Usage: scripts/docs-drift-scan.sh <crate-src-dir>...
set -euo pipefail
src=("$@"); [ ${#src[@]} -ge 1 ] || { echo "usage: $0 <src-dir>..." >&2; exit 2; }
cd "$(dirname "$0")/.."
grep -rnoE '\bautumn_(web|harvest|harvest_agent|harvest_plugin|harvest_sqlite)::[A-Za-z0-9_]+(::[A-Za-z0-9_]+)*' content | while IFS=: read -r f l p; do
  last="${p##*::}"
  [ "$last" = "prelude" ] && continue
  # `autumn_web::reexports::<crate>::<item>` names a third-party crate's item
  # (e.g. redis::RedisResult); only `<crate>` itself is defined in autumn-web.
  case "$p" in autumn_web::reexports::*::*) continue;; esac
  # `autumn_web::time` re-exports jiff types (`Timestamp`, `SignedDuration`).
  # `autumn_web::time::<Type>::<item>` names a jiff method, not autumn-web code.
  case "$p" in autumn_web::time::[A-Z]*::*) continue;; esac
  # Harvest 0.11.0 removed `autumn_harvest::chrono`; the upgrade notes name it
  # on purpose to say it is gone.
  [ "$p" = "autumn_harvest::chrono" ] && continue
  if ! grep -rqE "(fn|struct|enum|trait|mod|const|static|type|union|macro_rules!|use .*[ :{,])[[:space:]]*$last\b|as $last\b|\b$last[,;}]" "${src[@]}" --include='*.rs' 2>/dev/null; then
    echo "$f:$l: $p"
  fi
done | sort -u
