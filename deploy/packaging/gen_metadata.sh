#!/usr/bin/env bash
# Sync the workspace version from the root Cargo.toml into the packaging
# metadata (debian/control, rpm/wf-agent.spec). Run before packaging; CI
# should call this so package versions never drift from the crate version.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="$(awk -F= '/^\[workspace\.package\]/{f=1; next} /^\[/{f=0} f && $1 ~ /^version[[:space:]]*$/ {gsub(/[[:space:]]/, "", $2); gsub(/"/, "", $2); print $2; exit}' "$ROOT/Cargo.toml")"

if [[ -z "$VERSION" ]]; then
    echo "ERROR: workspace version not found in $ROOT/Cargo.toml" >&2
    exit 1
fi

sed -i "s/^Version: .*/Version: ${VERSION}/" \
    "$ROOT/deploy/packaging/debian/control" \
    "$ROOT/deploy/packaging/rpm/wf-agent.spec"

echo "Packaging metadata synced to version ${VERSION}"
