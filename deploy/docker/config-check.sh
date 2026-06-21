#!/bin/bash
# Validate the cross-service .env contract before the stack starts. Fails fast with a
# fix-it message (run as a one-shot service the others depend_on). Baked into the
# clapshot-server image as /usr/local/bin/clapshot-config-check.

URL="${CLAPSHOT_URL_BASE:-}"
[ -n "$URL" ] || { echo "CONFIG ERROR: CLAPSHOT_URL_BASE is not set." >&2; exit 1; }

scheme=${URL%%://*}
rest=${URL#*://}; hostport=${rest%%/*}; host=${hostport%%:*}

fail() { echo "CONFIG ERROR: $1" >&2; exit 1; }

# Cookies must match the public scheme (htwicket recipe only; var is set there).
if [ -n "${HTWICKET_INSECURE_COOKIES:-}" ]; then
    if [ "$scheme" = https ] && [ "$HTWICKET_INSECURE_COOKIES" = true ]; then
        echo "WARNING: https URL with HTWICKET_INSECURE_COOKIES=true — cookies will lack the Secure flag." >&2
    fi
    if [ "$scheme" = http ] && [ "$HTWICKET_INSECURE_COOKIES" != true ]; then
        fail "CLAPSHOT_URL_BASE is http:// but HTWICKET_INSECURE_COOKIES=$HTWICKET_INSECURE_COOKIES.
  Fix: set HTWICKET_INSECURE_COOKIES=true — Secure cookies are not sent over plain HTTP, so login would loop."
    fi
fi

# Caddy ACME/own-cert domain implies https and must match the URL's host.
if [ -n "${CADDY_CERT_DOMAIN:-}" ]; then
    [ "$scheme" = https ] || fail "CADDY_CERT_DOMAIN is set (HTTPS) but CLAPSHOT_URL_BASE is not https://.
  Fix: set CLAPSHOT_URL_BASE=https://$CADDY_CERT_DOMAIN/  — or unset CADDY_CERT_DOMAIN for plain HTTP."
    [ "$CADDY_CERT_DOMAIN" = "$host" ] || fail "CADDY_CERT_DOMAIN ($CADDY_CERT_DOMAIN) does not match the host in CLAPSHOT_URL_BASE ($host).
  Fix: make them equal."
fi

# Own-cert needs a hostname to serve it on.
if [ -n "${CADDY_TLS_CERT:-}" ] && [ -z "${CADDY_CERT_DOMAIN:-}" ]; then
    fail "CADDY_TLS_CERT is set but CADDY_CERT_DOMAIN is not.
  Fix: set CADDY_CERT_DOMAIN to the hostname the certificate is issued for."
fi

echo "config-check OK (scheme=$scheme host=$host)"
