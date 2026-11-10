#!/usr/bin/env bash
# Shared helpers for the headless end-to-end harness.
# All output of sourced functions goes through stdout/stderr explicitly;
# this file performs no actions when sourced.

set -u

# Repository root derived from this file location.
e2e_root() {
  local dir
  dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  printf '%s' "$dir"
}

# Fail with a message on stderr.
die() {
  echo "e2e: $*" >&2
  exit 1
}

# Require a file to exist.
require_file() {
  local path="$1"
  [ -f "$path" ] || die "missing required file: $path"
}

# Require a command to exist on PATH.
require_cmd() {
  local name="$1"
  command -v "$name" >/dev/null || die "$name is required"
}

# Wait until the health endpoint answers 200 or the deadline passes.
wait_for_health() {
  local base_url="$1"
  local deadline_secs="${2:-30}"
  local i
  for ((i = 0; i < deadline_secs; i++)); do
    if curl -fs -o /dev/null "$base_url/health" 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# Pick a free loopback port using python3.
free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}

# Print a section header for readable logs.
section() {
  echo "--- $* ---"
}

# Run one headless invocation with deterministic flags.
# Usage: run_headless <headless_bin> <remote> <directory> <out_file> <err_file> -- <args...>
run_headless() {
  local bin="$1" remote="$2" directory="$3" out_file="$4" err_file="$5"
  shift 5
  [ "$1" = "--" ] && shift
  "$bin" --remote "$remote" --directory "$directory" \
    --output json --no-color "$@" >"$out_file" 2>"$err_file"
}

# Assert that a JSON file carries .success == true (stdlib python3 only).
assert_success_envelope() {
  local file="$1"
  python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("success") is True else 1)' "$file" 2>/dev/null
}

# Extract `.data.status` from an execution envelope (empty when absent).
envelope_status_of() {
  local file="$1"
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])).get("data", {}); print(v.get("status", "") if isinstance(v, dict) else "")' "$file" 2>/dev/null
}

# Extract a top-level field from a JSON file (empty string when absent).
json_field() {
  local file="$1" field="$2"
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])).get(sys.argv[2], ""); print(v if isinstance(v,str) else ("" if v is None else json.dumps(v)))' "$file" "$field" 2>/dev/null
}

# Extract the execution id from an execution-run envelope: `.data` carries
# either the id string or an object holding `execution_id`.
execution_id_of() {
  local file="$1"
  python3 -c 'import json,sys; v=json.load(open(sys.argv[1])).get("data"); print(v.get("execution_id", "") if isinstance(v, dict) else (v if isinstance(v, str) else ""))' "$file" 2>/dev/null
}

# Normalize volatile fields so golden comparison stays stable.
normalize_json() {
  local src="$1" dest="$2"
  python3 - "$src" "$dest" <<'EOF'
import json, re, sys
pat = re.compile(r"timestamp|createdAt|created_at|updatedAt|updated_at|executionId|execution_id|traceId")
def clean(v):
    if isinstance(v, dict):
        return {k: clean(x) for k, x in v.items() if not pat.search(k)}
    if isinstance(v, list):
        return [clean(x) for x in v]
    return v
open(sys.argv[2], "w").write(json.dumps(clean(json.load(open(sys.argv[1]))), indent=2, sort_keys=True) + "\n")
EOF
}
