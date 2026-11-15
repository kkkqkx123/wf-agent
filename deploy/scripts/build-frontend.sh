#!/usr/bin/env bash
# Build the web frontend static bundle into apps/web-app/build/.
#
# Usage:
#   deploy/scripts/build-frontend.sh [--skip-install] [--preview] [--api-base-url URL]
#
# Same-origin nginx deployments (recommended: nginx proxies /api to the
# backend) need no --api-base-url and keep the default location.origin.
# Only split-domain deployments (pages and API on different hosts) bake
# the API origin in via VITE_API_BASE_URL and must tighten cors.toml.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FRONT="$ROOT/apps/web-app"
OUT="$FRONT/build"

SKIP_INSTALL=0
PREVIEW=0
API_BASE_URL=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-install) SKIP_INSTALL=1; shift ;;
        --preview) PREVIEW=1; shift ;;
        --api-base-url=*) API_BASE_URL="${1#--api-base-url=}"; shift ;;
        --api-base-url)
            API_BASE_URL="${2:?missing value for --api-base-url}"
            shift 2
            ;;
        -h|--help|help)
            sed -n '2,5p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -f "$FRONT/package.json" ]]; then
    echo "Error: frontend project not found at $FRONT" >&2
    exit 1
fi
if ! command -v npm >/dev/null 2>&1; then
    echo "Error: npm not found; install Node.js (>=22) first" >&2
    exit 1
fi

if [[ "$SKIP_INSTALL" -eq 0 ]]; then
    echo "==> Installing frontend dependencies ..."
    (cd "$FRONT" && npm ci)
else
    echo "==> Skipping dependency install (--skip-install)"
fi

echo "==> Building frontend ..."
if [[ -n "$API_BASE_URL" ]]; then
    echo "    VITE_API_BASE_URL=$API_BASE_URL (split-domain build)"
    export VITE_API_BASE_URL="$API_BASE_URL"
fi
if [[ "$PREVIEW" -eq 1 ]]; then
    (cd "$FRONT" && npm run build:preview)
else
    (cd "$FRONT" && npm run build)
fi

if [[ ! -f "$OUT/index.html" ]]; then
    echo "Error: frontend build produced no $OUT/index.html" >&2
    exit 1
fi
echo "==> Frontend build complete: $OUT"
