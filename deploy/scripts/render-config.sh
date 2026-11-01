#!/usr/bin/env bash
# Render deployment templates into their real paths on the target machine.
#
# One layer per invocation so personal templates can never land in the
# shared directory and vice versa:
#   render-config.sh --to managed  [--managed-dir /etc/wf]
#   render-config.sh --to settings [--settings-dir ~/.config/wf]
#   render-config.sh --to project  [--project-root .] [--with-project-overrides]
#
# Two placeholder kinds are handled differently:
# - @@NAME@@ placeholders (paths known at deploy time) are substituted
#   from --define NAME=VALUE or the surrounding environment.
# - ${VAR} / ${VAR:default} spans pass through untouched. The running
#   process expands them from its own environment (EnvironmentFile), so
#   secrets are never baked into rendered files on disk.
#
# Refuses to overwrite existing files unless --force is given.
# --dry-run prints every planned action without writing.

set -euo pipefail

TO=""
TEMPLATES=""
MANAGED_DIR="/etc/wf"
SETTINGS_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/wf"
PROJECT_ROOT="$PWD"
WITH_PROJECT_OVERRIDES=0
FORCE=0
DRY_RUN=0
DEFINES=()

usage() {
    cat >&2 <<EOF
usage: render-config.sh --to managed|settings|project [options]
  --templates DIR          template root (default: <deploy>/templates next to this script)
  --managed-dir PATH       managed dir for --to managed (default: /etc/wf)
  --settings-dir PATH      settings dir for --to settings (default: \$XDG_CONFIG_HOME/wf)
  --project-root PATH      project root for --to project (default: cwd)
  --define NAME=VALUE      value for @@NAME@@ placeholders (repeatable)
  --with-project-overrides also render infrastructure/ and server/ into .wf/ (project only)
  --force                  overwrite existing files
  --dry-run                print actions without writing
EOF
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --to) TO="$2"; shift 2 ;;
        --templates) TEMPLATES="$2"; shift 2 ;;
        --managed-dir) MANAGED_DIR="$2"; shift 2 ;;
        --settings-dir) SETTINGS_DIR="$2"; shift 2 ;;
        --project-root) PROJECT_ROOT="$2"; shift 2 ;;
        --define) DEFINES+=("$2"); shift 2 ;;
        --with-project-overrides) WITH_PROJECT_OVERRIDES=1; shift ;;
        --force) FORCE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        *) usage ;;
    esac
done

[[ "$TO" == "managed" || "$TO" == "settings" || "$TO" == "project" ]] || usage

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "$TEMPLATES" ]]; then
    TEMPLATES="$(cd "$SCRIPT_DIR/../templates" && pwd)"
fi
[[ -d "$TEMPLATES" ]] || { echo "error: templates dir missing: $TEMPLATES" >&2; exit 1; }

# Collect @@NAME@@ values: explicit --define entries win over the environment.
declare -A VARS
if ((${#DEFINES[@]})); then
    for def in "${DEFINES[@]}"; do
        name="${def%%=*}"
        VARS["$name"]="${def#*=}"
    done
fi

render_file() {
    local src="$1"
    local dst="$2"
    local mode="${3:-0644}"
    if [[ -e "$dst" && "$FORCE" == "0" ]]; then
        echo "skip (exists, use --force): $dst"
        return 0
    fi
    if [[ "$DRY_RUN" == "1" ]]; then
        echo "render: ${src#$TEMPLATES/} -> $dst ($mode)"
        return 0
    fi
    mkdir -p "$(dirname "$dst")"
    local tmp="$dst.tmp.$$"
    # Substitute @@NAME@@ only; ${...} spans pass through for the process
    # to expand from its own environment at startup.
    local content
    content="$(cat "$src")"
    local name
    for name in "${!VARS[@]}"; do
        content="${content//@@$name@@/${VARS[$name]}}"
    done
    printf '%s' "$content" > "$tmp"
    chmod "$mode" "$tmp"
    mv "$tmp" "$dst"
    echo "rendered: $dst"
}

render_tree() {
    local src_dir="$1"
    local dst_dir="$2"
    local mode="${3:-0644}"
    [[ -d "$src_dir" ]] || return 0
    while IFS= read -r -d '' src; do
        render_file "$src" "$dst_dir/${src#$src_dir/}" "$mode"
    done < <(find "$src_dir" -type f -print0)
}

case "$TO" in
    managed)
        render_tree "$TEMPLATES/infrastructure" "$MANAGED_DIR/infrastructure"
        render_tree "$TEMPLATES/server" "$MANAGED_DIR/server"
        # The managed directory acts as the service settings_dir, so it
        # carries the same live settings family the loader reads there.
        # llm-profiles/llm-providers stay out: import seeds only, the
        # gateway source of truth is the API/storage registry.
        render_file "$TEMPLATES/settings/config.toml" "$MANAGED_DIR/config.toml"
        render_file "$TEMPLATES/settings/mcp-settings.json" "$MANAGED_DIR/mcp-settings.json"
        render_file "$TEMPLATES/settings/skill-settings.json" "$MANAGED_DIR/skill-settings.json"
        render_tree "$TEMPLATES/settings/llm-auth" "$MANAGED_DIR/llm-auth"
        # Environment file carries the secrets (0600); rendered once with
        # empty values when absent, filled by ops on the target machine.
        if [[ ! -e "$MANAGED_DIR/wf-server.env" ]]; then
            render_file "$SCRIPT_DIR/../env/wf-server.env.template" "$MANAGED_DIR/wf-server.env" "0600"
        elif [[ "$FORCE" == "1" && "$DRY_RUN" == "0" ]]; then
            echo "skip: env file exists and holds live secrets, not overwritten even with --force"
        else
            echo "skip (exists, use --force): $MANAGED_DIR/wf-server.env"
        fi
        # Production needs every domain or an explicit runtime environment;
        # otherwise the process falls back to Development code defaults.
        if [[ "$DRY_RUN" == "0" ]]; then
            missing=0
            for domain in storage metrics output sandbox file-checkpoint tool-approval presets tools limits; do
                [[ -f "$MANAGED_DIR/infrastructure/$domain.toml" ]] || { echo "warning: domain not rendered: infrastructure/$domain.toml" >&2; missing=1; }
            done
            [[ "$missing" == "0" ]] || echo "warning: incomplete domain set falls back to Development defaults" >&2
        fi
        ;;
    settings)
        render_file "$TEMPLATES/settings/config.toml" "$SETTINGS_DIR/config.toml"
        render_file "$TEMPLATES/settings/mcp-settings.json" "$SETTINGS_DIR/mcp-settings.json"
        render_file "$TEMPLATES/settings/skill-settings.json" "$SETTINGS_DIR/skill-settings.json"
        render_tree "$TEMPLATES/settings/llm-auth" "$SETTINGS_DIR/llm-auth"
        render_tree "$TEMPLATES/settings/llm-profiles" "$SETTINGS_DIR/llm-profiles"
        render_tree "$TEMPLATES/settings/llm-providers" "$SETTINGS_DIR/llm-providers"
        render_tree "$TEMPLATES/server" "$SETTINGS_DIR/server"
        ;;
    project)
        # Render every family present under templates/project (mcp/skills
        # plus the resource index families as import starting points, never
        # as live config: the loader auto-reads only mcp/skills here).
        # infrastructure/server stay opt-in so business repos never inherit
        # ops tuning by accident.
        render_tree "$TEMPLATES/project" "$PROJECT_ROOT/.wf"
        if [[ "$WITH_PROJECT_OVERRIDES" == "1" ]]; then
            render_tree "$TEMPLATES/infrastructure" "$PROJECT_ROOT/.wf/infrastructure"
            render_tree "$TEMPLATES/server" "$PROJECT_ROOT/.wf/server"
        fi
        ;;
esac

echo "done: --to $TO"
