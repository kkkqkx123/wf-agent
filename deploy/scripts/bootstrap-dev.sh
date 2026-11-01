#!/usr/bin/env bash
# Development bootstrap: render just enough configuration for a local run.
#
# Uses Development code defaults wherever possible and only fills the gaps
# that need explicit files. Writes into a local directory (never a system
# location) and prints the follow-up server command.
#
# Usage: bootstrap-dev.sh [--dev-dir ./dev-config] [--project-root .]

set -euo pipefail

DEV_DIR="./dev-config"
PROJECT_ROOT="$PWD"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dev-dir) DEV_DIR="$2"; shift 2 ;;
        --project-root) PROJECT_ROOT="$2"; shift 2 ;;
        *) echo "usage: bootstrap-dev.sh [--dev-dir PATH] [--project-root PATH]" >&2; exit 2 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"$SCRIPT_DIR/render-config.sh" --to settings \
    --settings-dir "$DEV_DIR"

echo ""
echo "dev config ready in: $DEV_DIR"
echo "next: wf-server --config $PROJECT_ROOT --settings-dir $DEV_DIR"
