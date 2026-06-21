#!/usr/bin/env bash
# Smoke-test a Compose recipe end-to-end. Builds the clapshot-server/web images locally
# (single-arch), brings the stack up, runs Playwright inside the official Playwright
# container, then tears everything down.
#
#   ./run.sh                 # default recipe: htwicket
#   SKIP_BUILD=1 ./run.sh    # reuse already-built clapshot-{server,web}:latest
#   PLAYWRIGHT_IMAGE=mcr.microsoft.com/playwright:vX-noble ./run.sh   # pin a known/digest tag
set -euo pipefail

RECIPE="${1:-htwicket}"
TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$TESTS_DIR/../../.." && pwd)"
RECIPE_DIR="$REPO/deploy/compose/$RECIPE"
ENV_FILE="$TESTS_DIR/$RECIPE.env"
GHCR_BASE="ghcr.io/elonen"

[ -f "$RECIPE_DIR/compose.yml" ] || { echo "No such recipe: $RECIPE_DIR" >&2; exit 2; }
[ -f "$ENV_FILE" ] || { echo "No test env: $ENV_FILE (only 'htwicket' exists so far)" >&2; exit 2; }

export TESTS_DIR
export ASSET_FILE="$REPO/server/src/tests/assets/60fps-example.mp4"
export PLAYWRIGHT_IMAGE="${PLAYWRIGHT_IMAGE:-mcr.microsoft.com/playwright:v1.61.0-noble}"
[ -f "$ASSET_FILE" ] || { echo "Missing test asset: $ASSET_FILE" >&2; exit 2; }

# --- Build the two service images locally (single-arch -> loadable; the multi-arch Makefile
#     targets can't `docker load`). Needs the .debs from `make debian-docker`. ----------------
if [ "${SKIP_BUILD:-}" != "1" ]; then
  compgen -G "$REPO/dist_deb/*_trixie_*.deb" >/dev/null \
    || { echo "No trixie .debs in dist_deb/ — run 'make debian-docker' first." >&2; exit 2; }
  echo "=== Building clapshot-server + clapshot-web (local arch) ==="
  docker build -t $GHCR_BASE/clapshot-server:latest -f "$REPO/deploy/docker/clapshot-server.Dockerfile" "$REPO"
  docker build -t $GHCR_BASE/clapshot-web:latest    -f "$REPO/deploy/docker/clapshot-web.Dockerfile"    "$REPO"
fi

COMPOSE=(docker compose
  --project-name "clapshot-test-$RECIPE"
  --env-file "$ENV_FILE"
  -f "$RECIPE_DIR/compose.yml"
  -f "$TESTS_DIR/compose.test.yml")

cleanup() { "${COMPOSE[@]}" down -v --remove-orphans >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "=== Bringing up the '$RECIPE' stack ==="
"${COMPOSE[@]}" up -d

echo "=== Running Playwright ($PLAYWRIGHT_IMAGE) ==="
set +e
"${COMPOSE[@]}" run --rm playwright
rc=$?
set -e

if [ "$rc" -ne 0 ]; then
  echo "=== FAILED (exit $rc) — recent stack logs ===" >&2
  "${COMPOSE[@]}" logs --no-color --tail=40 clapshot-server clapshot-web htwicket htwicket-init config-check 2>&1 | tail -80 >&2 || true
fi
exit "$rc"
