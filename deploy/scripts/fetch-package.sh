#!/usr/bin/env bash
# Download a versioned wf-agent distribution package.
#
# The download location is not decided yet. WF_PACKAGE_BASE_URL is a
# placeholder anchor: override it (or pass --base-url) once the package
# hosting is confirmed. Layout contract per module:
#   wf-agent-server-<version>-linux-<arch>.tar.gz  (wf-server + web/)
#   wf-agent-cli-<version>-linux-<arch>.tar.gz     (wf wf-mini wf-headless wf-debug)
# each with a sibling .sha256 checksum file.
#
# Usage:
#   fetch-package.sh --module server|cli|all [--version X] [--dest DIR]
#                    [--base-url URL] [--allow-missing-checksum]
#
# Checksum policy is strict by default: a present checksum must verify,
# a missing checksum fails unless --allow-missing-checksum is given.

set -euo pipefail

MODULE=""
VERSION=""
DEST="./dist"
BASE_URL="${WF_PACKAGE_BASE_URL:-https://packages.example.com/wf-agent}"
ALLOW_MISSING_CHECKSUM=0

usage() {
    cat >&2 <<EOF
usage: fetch-package.sh --module server|cli|all [options]
  --version VERSION        package version (default: workspace Cargo.toml)
  --dest DIR               extraction directory (default: ./dist)
  --base-url URL           package base URL (default: \$WF_PACKAGE_BASE_URL placeholder)
  --allow-missing-checksum proceed with a warning when no .sha256 is published
EOF
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --module) MODULE="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --dest) DEST="$2"; shift 2 ;;
        --base-url) BASE_URL="$2"; shift 2 ;;
        --allow-missing-checksum) ALLOW_MISSING_CHECKSUM=1; shift ;;
        *) usage ;;
    esac
done

[[ "$MODULE" == "server" || "$MODULE" == "cli" || "$MODULE" == "all" ]] || usage

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ -z "$VERSION" ]]; then
    VERSION="$(awk -F= '/^\[workspace\.package\]/{f=1; next} /^\[/{f=0} f && $1 ~ /^version[[:space:]]*$/ {gsub(/[[:space:]]/, "", $2); gsub(/"/, "", $2); print $2; exit}' "$REPO_ROOT/Cargo.toml")"
fi
if [[ -z "$VERSION" ]]; then
    echo "error: cannot determine version; pass --version explicitly" >&2
    exit 1
fi

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64) ARCH="x86_64" ;;
    aarch64) ARCH="aarch64" ;;
    *)
        echo "error: unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

MODULES=()
if [[ "$MODULE" == "all" ]]; then
    MODULES=(server cli)
else
    MODULES=("$MODULE")
fi

mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for mod in "${MODULES[@]}"; do
    name="wf-agent-${mod}-${VERSION}-linux-${ARCH}.tar.gz"
    url="${BASE_URL%/}/${name}"
    echo "fetching: $url"
    if ! curl -fsSL -o "$TMP/$name" "$url"; then
        echo "error: download failed: $url" >&2
        echo "hint: the package base URL is a placeholder until hosting is confirmed;" >&2
        echo "hint: set WF_PACKAGE_BASE_URL or install from local build output instead" >&2
        exit 1
    fi
    if curl -fsSL -o "$TMP/$name.sha256" "${url}.sha256" 2>/dev/null; then
        (cd "$TMP" && sha256sum -c "$name.sha256") || {
            echo "error: checksum mismatch for $name" >&2
            exit 1
        }
    elif [[ "$ALLOW_MISSING_CHECKSUM" == "1" ]]; then
        echo "warning: no checksum published for $name; proceeding unverified" >&2
    else
        echo "error: no checksum published for $name (pass --allow-missing-checksum to override)" >&2
        exit 1
    fi
    tar -xzf "$TMP/$name" -C "$DEST"
    echo "extracted: $name -> $DEST"
done

echo "done: module=$MODULE version=$VERSION dest=$DEST"
