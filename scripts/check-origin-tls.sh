#!/usr/bin/env bash
# Origin TLS check: does Fly still hold a valid certificate for each custom
# domain Cloudflare proxies to it?
#
# Cloudflare reaches the origin by sending the *custom* hostname as SNI to
# Fly's edge, so the certificate that matters is the one Fly holds for
# `autumn-web.app` — not the `*.fly.dev` one, which is always fine. When Fly's
# copy lapses, Fly's edge stops completing the handshake for that name and
# every visitor gets Cloudflare's `525 SSL handshake failed`, while
# `autumn-io.fly.dev` keeps answering 200. That happened on 2026-10-09: the
# certificate expired because neither ACME challenge could reach Fly through
# Cloudflare, and nothing noticed until the site was down. See
# docs/runbooks/cloudflare-525.md.
#
# This asks Fly's edge directly, bypassing Cloudflare, so a renewal that is
# failing shows up while the old certificate still has weeks left.
#
# Usage: scripts/check-origin-tls.sh [<hostname>...]   (default: autumn-web.app)
# Env:   ORIGIN    host[:port] to connect to       (default: autumn-io.fly.dev:443)
#        MIN_DAYS  fail if the cert expires sooner  (default: 14)
#        ORIGIN_TRUST  which CA the origin cert must chain to:
#                  public               - publicly trusted, e.g. Fly's Let's
#                                         Encrypt certs (default)
#                  cloudflare-origin-ca - Cloudflare Origin CA, for an imported
#                                         origin certificate; uses the roots in
#                                         scripts/cloudflare-origin-ca.pem
#        CAFILE    explicit trust store; overrides ORIGIN_TRUST
#        TIMEOUT   seconds per handshake            (default: 30)
set -euo pipefail

origin="${ORIGIN:-autumn-io.fly.dev:443}"
[[ "$origin" == *:* ]] || origin="$origin:443"
min_days="${MIN_DAYS:-14}"
timeout_secs="${TIMEOUT:-30}"
hosts=("$@")
[ ${#hosts[@]} -ge 1 ] || hosts=(autumn-web.app)

cafile="${CAFILE:-}"
case "${ORIGIN_TRUST:-public}" in
  public) ;;
  cloudflare-origin-ca)
    [ -n "$cafile" ] || cafile="$(dirname "$0")/cloudflare-origin-ca.pem" ;;
  *)
    echo "ORIGIN_TRUST must be 'public' or 'cloudflare-origin-ca', got '$ORIGIN_TRUST'" >&2
    exit 2 ;;
esac
verify_args=()
[ -n "$cafile" ] && verify_args=(-CAfile "$cafile")

status=0
for host in "${hosts[@]}"; do
  # No `-verify_return_error`: fetch whatever is presented, then judge it, so
  # "no certificate at all" and "certificate is wrong" read differently.
  # `timeout` because s_client has no TLS handshake timeout of its own (its
  # `-timeout` is DTLS-only): a peer that accepts TCP and then stalls would
  # otherwise hang the check until the runner kills it, hours later.
  out="$(timeout "$timeout_secs" openssl s_client -connect "$origin" -servername "$host" \
    -verify_hostname "$host" "${verify_args[@]}" </dev/null 2>/dev/null || true)"
  cert="$(openssl x509 <<<"$out" 2>/dev/null || true)"

  if [ -z "$cert" ]; then
    echo "FAIL $host: $origin presented no certificate for this name." \
      "Cloudflare will answer 525. Is the Fly certificate missing or expired?" >&2
    status=1
    continue
  fi

  not_after="$(openssl x509 -noout -enddate <<<"$cert" | cut -d= -f2)"
  verify="$(sed -n 's/^ *Verify return code: \([0-9]*\).*/\1/p' <<<"$out" | tail -1)"

  if [ "${verify:-1}" != "0" ]; then
    echo "FAIL $host: certificate does not verify" \
      "($(grep -m1 'Verify return code' <<<"$out" | sed 's/^ *//')); expires $not_after." >&2
    status=1
  elif ! openssl x509 -noout -checkend $((min_days * 86400)) <<<"$cert" >/dev/null; then
    echo "FAIL $host: certificate expires $not_after, within $min_days days." \
      "Fly renews ~30 days out, so renewal is failing." >&2
    status=1
  else
    echo "ok   $host: valid until $not_after"
  fi
done
exit "$status"
