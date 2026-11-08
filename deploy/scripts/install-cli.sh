#!/usr/bin/env bash
# Install wf-agent CLI binaries for the current user (no root required).
#
# Installs wf, wf-mini, wf-headless and wf-debug to $PREFIX/bin.
# This script never touches services or system directories; personal
# settings and project config keep using render-config.sh
# (--to settings / --to project) and bootstrap-dev.sh separately.
#
# Usage: ./install-cli.sh

set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
FETCH="${FETCH:-no}"                   # yes: download via fetch-package.sh first
PACKAGE_VERSION="${PACKAGE_VERSION:-}" # default: workspace version
BIN_SRC_DIR="${BIN_SRC_DIR:-}"         # artifact dir (local build or fetched)

normalize() {
    local p="${1%/}"
    printf '%s\n' "${p:-/}"
}
PREFIX="$(normalize "$PREFIX")"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CLI_BINS=(wf wf-mini wf-headless wf-debug)

FETCH_DEST=""
if [[ "$FETCH" == "yes" ]]; then
    FETCH_DEST="$(mktemp -d)"
    trap 'rm -rf "$FETCH_DEST"' EXIT
    version_args=()
    if [[ -n "$PACKAGE_VERSION" ]]; then
        version_args=(--version "$PACKAGE_VERSION")
    fi
    "$SCRIPT_DIR/fetch-package.sh" --module cli \
        --dest "$FETCH_DEST" "${version_args[@]}"
    BIN_SRC_DIR="$FETCH_DEST"
fi

if [[ -z "$BIN_SRC_DIR" ]]; then
    for candidate in "$REPO_ROOT/bin" "$REPO_ROOT/target/release" "$REPO_ROOT" "./bin" "."; do
        if [[ -x "$candidate/wf" ]]; then
            BIN_SRC_DIR="$candidate"
            break
        fi
    done
fi
if [[ -z "$BIN_SRC_DIR" ]]; then
    echo "error: no CLI artifacts found; build first or rerun with FETCH=yes" >&2
    exit 1
fi

install -d -m 0755 "$PREFIX/bin"
for bin in "${CLI_BINS[@]}"; do
    src="$BIN_SRC_DIR/$bin"
    if [[ ! -x "$src" ]]; then
        echo "WARNING: $bin not found in $BIN_SRC_DIR; skipped" >&2
        continue
    fi
    install -m 0755 "$src" "$PREFIX/bin/$bin"
    echo "Installed $PREFIX/bin/$bin"
done

case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "note: $PREFIX/bin is not on PATH; export PATH=\"$PREFIX/bin:\$PATH\"" ;;
esac

echo "done."
echo "Next (optional): render-config.sh --to settings --settings-dir <dir>"
