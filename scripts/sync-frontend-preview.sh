#!/bin/bash
# sync-frontend-preview.sh
# Sync source files from apps/web-app to apps/web-app-preview.
# Preserves preview-only files: fixtures, mock client, and .env.
#
# The preview project is a near-mirror of web-app that runs entirely
# against local fixture data instead of a live backend. Every route,
# component, store, service, and type comes from web-app — only the
# API client is swapped out for a fixture-backed implementation.
 
set -euo pipefail
 
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$ROOT_DIR/apps/web-app"
DST_DIR="$ROOT_DIR/apps/web-app-preview"
 
# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
 
die() { echo "Error: $*" >&2; exit 1; }
info() { echo "==> $*"; }
 
require_dir() {
[ -d "$1" ] || die "Directory not found: $1 ($2)"
}
 
require_cmd() {
command -v "$1" >/dev/null 2>&1 || die "Required command not installed: $1 (for $2)"
}
 
# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
 
require_dir "$SRC_DIR" "source (apps/web-app)"
require_dir "$DST_DIR" "target (apps/web-app-preview)"
 
require_cmd rsync "incremental file sync"
require_cmd node "package.json dep merge (no jq dependency)"
 
# ---------------------------------------------------------------------------
# Resolve preview-only markers (must exist so we never delete them)
# ---------------------------------------------------------------------------
 
# These paths are relative to DST_DIR. The script preserves them by
# excluding them from the rsync mirror pass and never deleting them.
PREVIEW_ONLY_PATHS=(
"src/lib/fixtures/"       # all local sample data
".env"                    # preview env overrides (VITE_USE_MOCK etc.)
)
 
# client.ts is special: on first run the preview doesn't have its own
# fixture-backed version yet, so we seed it from source first. Once the
# preview replaces it with its own implementation, we start excluding it
# from rsync so future syncs don't clobber the mock client.
if [ ! -f "$DST_DIR/src/lib/api/client.ts" ]; then
info "Seeding src/lib/api/client.ts from web-app (no preview version yet)"
mkdir -p "$DST_DIR/src/lib/api"
cp "$SRC_DIR/src/lib/api/client.ts" "$DST_DIR/src/lib/api/client.ts"
else
PREVIEW_ONLY_PATHS+=("src/lib/api/client.ts")
fi
 
# ---------------------------------------------------------------------------
# Step 1 — Mirror source tree, excluding preview-only files
# ---------------------------------------------------------------------------
 
info "Mirroring source tree from apps/web-app → apps/web-app-preview"
 
# Build rsync exclude arguments from the preview-only list.
RSYNC_EXCLUDES=()
for rel in "${PREVIEW_ONLY_PATHS[@]}"; do
RSYNC_EXCLUDES+=(--exclude="$rel")
done
 
# Also exclude node_modules, dist, .svelte-kit, and any editor noise that
# might have leaked into the source tree. These are build artifacts, not
# source, and should never be copied between projects.
#
# src/lib/api/schema.d.ts is deliberately NOT excluded: the synced runtime
# modules (envelope.ts, services/graph.ts, services/workflow-locks.ts) import
# types from it, so the preview must type-check against the same contract
# declarations as the main app. Only the API client stays fixture-backed.
RSYNC_EXCLUDES+=(
--exclude="package.json"   # built by the merge step below
--exclude="node_modules/"
--exclude="dist/"
--exclude="build/"
--exclude=".svelte-kit/"
--exclude=".env"
--exclude=".env.*"
--exclude=".DS_Store"
--exclude=".vscode/"
--exclude=".idea/"
)
 
# rsync with --delete so removed files in the source are also removed in
# the preview — except for preview-only files, which the excludes protect.
# Trailing slashes on both paths are significant: we want the *contents*
# of SRC_DIR mirrored into DST_DIR, not SRC_DIR itself becoming a subdir.
rsync -av --delete "${RSYNC_EXCLUDES[@]}" "$SRC_DIR/" "$DST_DIR/"
 
# ---------------------------------------------------------------------------
# Step 2 — Merge package.json (preserve preview identity, sync deps & scripts)
# ---------------------------------------------------------------------------
 
info "Merging package.json (preview name preserved, deps synced from web-app)"
 
# If the preview has no package.json yet (first run), create a minimal shell
# so the merge logic below has identity fields to preserve.
if [ ! -f "$DST_DIR/package.json" ]; then
cat > "$DST_DIR/package.json" <<'INIT'
{
"name": "@wf-agent/web-app-preview",
"version": "0.1.0",
"description": "Preview variant of web-app backed by local fixture data"
}
INIT
fi
 
# Capture preview's identity fields before we overwrite anything.
read_pkg_field() {
node -p 'const pkg=JSON.parse(require("node:fs").readFileSync(process.argv[1],"utf8")); pkg[process.argv[2]] ?? ""' "$DST_DIR/package.json" "$1"
}
PREVIEW_NAME=$(read_pkg_field name)
PREVIEW_VERSION=$(read_pkg_field version)
PREVIEW_DESC=$(read_pkg_field description)
 
# Merge strategy:
#   - name / version / description → keep preview identity
#   - scripts, devDependencies, dependencies → take from source (so new
#     scripts or packages added to web-app propagate here)
#   - everything else (keywords, author, license, engines) → keep preview's
#     existing value *or* fall back to source if preview doesn't define it.
#
# jq takes two objects and lets us spell out the merge explicitly. No
# "deep merge" magic — we control which keys win so this stays readable.
 
# jq is not available in all environments; use node (required for the
# frontend toolchain anyway) to build the merged package.json.
node "$SCRIPT_DIR/sync-frontend-preview-merge-pkg.cjs" \
"$SRC_DIR/package.json" \
"$DST_DIR/package.json" \
"$PREVIEW_NAME" \
"$PREVIEW_VERSION" \
"$PREVIEW_DESC"
 
mv "$DST_DIR/package.json.tmp" "$DST_DIR/package.json"

# The preview imports shared UI primitives (@wf-agent/ui) exactly like the
# source app does. web-app declares it as "file:../ui", which resolves to the
# workspace package under the apps/ npm workspace. Re-inject it here
# explicitly so the dependency survives even if the source app ever drops it.
PREVIEW_HAS_UI=$(node -p 'JSON.parse(require("node:fs").readFileSync(process.argv[1],"utf8")).dependencies?.["@wf-agent/ui"] ?? ""' "$DST_DIR/package.json")
if [ -z "$PREVIEW_HAS_UI" ]; then
info "Injecting missing @wf-agent/ui dependency into preview package.json"
node -e '
const fs = require("node:fs");
const path = process.argv[1];
const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
pkg.dependencies = pkg.dependencies || {};
pkg.dependencies["@wf-agent/ui"] = "file:../ui";
fs.writeFileSync(path + ".tmp", JSON.stringify(pkg, null, "\t") + "\n");
fs.renameSync(path + ".tmp", path);
' "$DST_DIR/package.json"
fi
 
# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
 
echo
info "Sync complete."
echo
echo "Mirrored from apps/web-app:"
echo "  - package.json                (deps & scripts synced; name/version/description preserved)"
echo "  - .gitignore, .prettierrc*, eslint.config.js"
echo "  - svelte.config.js, tsconfig*.json, vite.config.ts (test config included)"
echo "  - src/app.html, src/app.css"
echo "  - src/lib/api/envelope.ts"
echo "  - src/lib/api/schema.d.ts     (generated contract types)"
echo "  - src/lib/components/**/*  src/lib/config/**/*  src/lib/services/**/*"
echo "  - src/lib/stores/**/*      src/lib/types/**/*    src/lib/utils/**/*"
echo "  - src/routes/**/*"
echo
echo "Preserved in apps/web-app-preview (not overwritten):"
echo "  - src/lib/api/client.ts   (fixture-backed client)"
echo "  - src/lib/fixtures/**/*   (sample data)"
echo "  - .env                    (preview env vars)"
echo
echo "Next steps:"
echo "  cd apps/web-app-preview && npm install"
echo "  npm run dev"