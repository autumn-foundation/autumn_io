#!/usr/bin/env bash
# Folio drift scan: every `autumn_web::...` path in content/ whose final
# segment is not defined/re-exported anywhere in the pinned autumn-web source.
# Usage: scripts/docs-drift-scan.sh <autumn-web-src-dir> [<autumn-macros-src-dir>]
set -euo pipefail
src=("$@"); [ ${#src[@]} -ge 1 ] || { echo "usage: $0 <src-dir>..." >&2; exit 2; }
cd "$(dirname "$0")/.."
grep -rnoE 'autumn_web::[A-Za-z0-9_]+(::[A-Za-z0-9_]+)*' content | while IFS=: read -r f l p; do
  last="${p##*::}"
  [ "$last" = "prelude" ] && continue
  if ! grep -rqE "(fn|struct|enum|trait|mod|const|static|type|union|macro_rules!|use .*[ :{,])[[:space:]]*$last\b|as $last\b|\b$last[,;}]" "${src[@]}" --include='*.rs' 2>/dev/null; then
    echo "$f:$l: $p"
  fi
done | sort -u
