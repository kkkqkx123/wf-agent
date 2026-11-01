#!/usr/bin/env bash
# Sync deployment templates from the canonical configuration sources.
#
# One-way only: sources -> deploy/templates/. Never writes back to the
# sources. Files under deploy/templates/ are generated; edit the source
# and re-run this script instead of hand-editing the output.
#
# Usage:
#   scripts/sync-deploy.sh          # copy sources into deploy/templates/
#   scripts/sync-deploy.sh --check  # fail on any drift (CI mode, no writes)
#
# Rules:
# - Fixed filenames (infrastructure/, server/, index.json files, .wf/*.json,
#   llm-providers/, .wf-user/config.toml) copy byte-identical.
# - Draft samples whose name contains ".example." copy with that segment
#   removed so the output is a loadable filename (loaders skip ".example."
#   names, so samples must be renamed before they can take effect).
# - Markdown docs, the plugins/ contract tree (samples plus build output),
#   credential stores and local state are never synced.

set -euo pipefail

CHECK=0
if [[ "${1:-}" == "--check" ]]; then
    CHECK=1
elif [[ "${1:-}" != "" ]]; then
    echo "usage: sync-deploy.sh [--check]" >&2
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATES="$REPO_ROOT/deploy/templates"

DRIFT=0
PRODUCED_LIST="$(mktemp)"
trap 'rm -f "$PRODUCED_LIST"' EXIT

report() {
    if [[ "$CHECK" == "1" ]]; then
        echo "drift: $1" >&2
        DRIFT=1
    else
        echo "$1"
    fi
}

# Sync one file. Strips a single ".example" segment from the destination
# name when present (mcp-settings.example.json -> mcp-settings.json).
sync_file() {
    local src="$1"
    local dst="$2"
    echo "$dst" >> "$PRODUCED_LIST"
    if [[ "$CHECK" == "1" ]]; then
        if [[ ! -f "$dst" ]] || ! cmp -s "$src" "$dst"; then
            report "file differs or missing: ${dst#$REPO_ROOT/} <- ${src#$REPO_ROOT/}"
        fi
        return 0
    fi
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
}

# Sync one directory pair, preserving structure and skipping markdown.
sync_dir() {
    local src_dir="$1"
    local dst_dir="$2"
    if [[ ! -d "$src_dir" ]]; then
        echo "warning: source dir missing, skipped: ${src_dir#$REPO_ROOT/}" >&2
        return 0
    fi
    while IFS= read -r -d '' src; do
        local rel="${src#$src_dir/}"
        local base="$(basename "$rel")"
        # Draft samples become loadable names by dropping ".example".
        local dst_base="${base//.example./.}"
        local rel_dir="$(dirname "$rel")"
        local dst
        if [[ "$rel_dir" == "." ]]; then
            dst="$dst_dir/$dst_base"
        else
            dst="$dst_dir/$rel_dir/$dst_base"
        fi
        sync_file "$src" "$dst"
    done < <(find "$src_dir" -type f ! -name '*.md' -print0)
}

# Directory pairs: <source> <destination>. Structure is preserved so
# index.json relative path patterns keep resolving.
DIR_PAIRS=(
    "config-examples/infrastructure|deploy/templates/infrastructure"
    "config-examples/server|deploy/templates/server"
    ".wf-user/llm-auth|deploy/templates/settings/llm-auth"
    ".wf-user/llm-profiles|deploy/templates/settings/llm-profiles"
    ".wf-user/llm-providers|deploy/templates/settings/llm-providers"
    "config-examples/workflows|deploy/templates/project/workflows"
    "config-examples/agent-loops|deploy/templates/project/agent-loops"
    "config-examples/node-templates|deploy/templates/project/node-templates"
    "config-examples/prompt-templates|deploy/templates/project/prompt-templates"
    "config-examples/scripts|deploy/templates/project/scripts"
)

for pair in "${DIR_PAIRS[@]}"; do
    sync_dir "$REPO_ROOT/${pair%%|*}" "$REPO_ROOT/${pair##*|}"
done

# Single-file mappings whose destination name differs from the source.
sync_file "$REPO_ROOT/.wf-user/config.toml" "$TEMPLATES/settings/config.toml"
sync_file "$REPO_ROOT/config-examples/mcp/mcp-settings.example.json" "$TEMPLATES/settings/mcp-settings.json"
sync_file "$REPO_ROOT/config-examples/skills/skill-settings.example.json" "$TEMPLATES/settings/skill-settings.json"
sync_file "$REPO_ROOT/.wf/mcp.json" "$TEMPLATES/project/mcp.json"
sync_file "$REPO_ROOT/.wf/skills.json" "$TEMPLATES/project/skills.json"

# Sweep stale outputs: anything under deploy/templates/ that no mapping
# produced is a leftover of a renamed source and must go.
if [[ -d "$TEMPLATES" ]]; then
    while IFS= read -r -d '' existing; do
        if ! grep -qxF "$existing" "$PRODUCED_LIST"; then
            if [[ "$CHECK" == "1" ]]; then
                report "stale file without source: ${existing#$REPO_ROOT/}"
            else
                echo "remove stale: ${existing#$REPO_ROOT/}"
                rm -f "$existing"
            fi
        fi
    done < <(find "$TEMPLATES" -type f -print0)
    # Drop directories left empty by the sweep (normal mode only).
    if [[ "$CHECK" == "0" ]]; then
        find "$TEMPLATES" -type d -empty -delete 2>/dev/null || true
    fi
fi

if [[ "$CHECK" == "1" ]]; then
    if [[ "$DRIFT" == "1" ]]; then
        echo "deploy templates are out of sync; run scripts/sync-deploy.sh" >&2
        exit 1
    fi
    echo "deploy templates are in sync"
else
    echo "synced sources -> deploy/templates/"
fi
