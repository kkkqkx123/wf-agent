#!/usr/bin/env bash
# Copy freshly built server and headless binaries into e2e/bin.
# Default mode only copies; pass --build to compile first.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$ROOT/e2e/bin"
DO_BUILD=0

for arg in "$@"; do
  case "$arg" in
    --build) DO_BUILD=1 ;;
    --help|-h)
      echo "usage: sync-bin.sh [--build]"
      echo "copies target/debug/wf-server and target/debug/wf-headless into e2e/bin"
      exit 0
      ;;
    *) echo "e2e: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [ "$DO_BUILD" -eq 1 ]; then
  echo "e2e: building wf-server and wf-headless"
  (cd "$ROOT" && cargo build -p wf-server -p wf-headless)
fi

mkdir -p "$BIN_DIR"

for name in wf-server wf-headless; do
  src="$ROOT/target/debug/$name"
  [ -f "$src" ] || { echo "e2e: missing build artifact: $src (run with --build)" >&2; exit 1; }
  cp -f "$src" "$BIN_DIR/$name"
  chmod +x "$BIN_DIR/$name"
done

echo "e2e: synced binaries:"
"$BIN_DIR/wf-server" --version
"$BIN_DIR/wf-headless" --version
