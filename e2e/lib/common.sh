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

# Assert that a JSON file carries .success == true.
assert_success_envelope() {
  local file="$1"
  jq -e '.success == true' "$file" >/dev/null 2>&1
}

# Normalize volatile fields so golden comparison stays stable.
normalize_json() {
  local src="$1" dest="$2"
  jq 'walk(if type == "object" then with_entries(select(.key | test("timestamp|createdAt|created_at|updatedAt|updated_at|executionId|execution_id|traceId") | not)) else . end)' \
    "$src" >"$dest"
}
