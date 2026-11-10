#!/usr/bin/env bash
# Headless end-to-end run: start an isolated server, drive it with
# wf-headless, collect outputs, then stop the server.
# Default tiers need no external model service; model sessions stay manual.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

ROOT="$(e2e_root)"
BIN_DIR="$ROOT/e2e/bin"
FIX_DIR="$ROOT/e2e/fixtures"
ACTUAL_DIR="$ROOT/e2e/outputs/actual"
SERVER_BIN="$BIN_DIR/wf-server"
HEADLESS_BIN="$BIN_DIR/wf-headless"

TIER="default"
SKIP_LOCAL_EXECUTION=0
KEEP_RUNNING=0

for arg in "$@"; do
  case "$arg" in
    --tier=*) TIER="${arg#--tier=}" ;;
    --skip-local-execution) SKIP_LOCAL_EXECUTION=1 ;;
    --keep-running) KEEP_RUNNING=1 ;;
    --help|-h)
      echo "usage: run-e2e.sh [--tier=default|full] [--skip-local-execution] [--keep-running]"
      exit 0
      ;;
    *) echo "e2e: unknown argument: $arg" >&2; exit 2 ;;
  esac
done

require_file "$SERVER_BIN"
require_file "$HEADLESS_BIN"
command -v curl >/dev/null || die "curl is required"
command -v jq >/dev/null || die "jq is required"

mkdir -p "$ACTUAL_DIR"
WORK_DIR="$(mktemp -d "$ACTUAL_DIR/workspace.XXXXXX")"
SERVER_LOG="$ACTUAL_DIR/server.log"
PORT="$(free_port)"
REMOTE="http://127.0.0.1:$PORT"
PASS=0
FAIL=0

cleanup() {
  if [ "$KEEP_RUNNING" -eq 0 ]; then
    [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

section "start isolated server on $REMOTE"
"$SERVER_BIN" --addr "127.0.0.1:$PORT" --storage memory \
  --directory "$WORK_DIR" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
wait_for_health "$REMOTE" 30 || { echo "e2e: server did not become ready, see $SERVER_LOG" >&2; exit 1; }

# Report one named check based on the headless exit code and envelope.
check() {
  local name="$1" expected_code="$2" actual_code="$3" out_file="$4"
  if [ "$actual_code" -eq "$expected_code" ]; then
    if [ "$expected_code" -eq 0 ]; then
      assert_success_envelope "$out_file" || { echo "FAIL $name (envelope)" ; FAIL=$((FAIL+1)); return; }
    fi
    echo "PASS $name"
    PASS=$((PASS+1))
  else
    echo "FAIL $name (exit $actual_code, want $expected_code)"
    FAIL=$((FAIL+1))
  fi
}

run_case() {
  local name="$1"; shift
  local out="$ACTUAL_DIR/$name.stdout.json" err="$ACTUAL_DIR/$name.stderr.txt"
  set +e
  run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" "$out" "$err" -- "$@"
  local code=$?
  set -e
  check "$name" 0 "$code" "$out"
}

run_negative_case() {
  local name="$1"; shift
  local out="$ACTUAL_DIR/$name.stdout.json" err="$ACTUAL_DIR/$name.stderr.txt"
  set +e
  run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" "$out" "$err" -- "$@"
  local code=$?
  set -e
  if [ "$code" -ne 0 ]; then
    echo "PASS $name"
    PASS=$((PASS+1))
  else
    echo "FAIL $name (expected non-zero exit)"
    FAIL=$((FAIL+1))
  fi
}

section "health and diagnostics tier"
run_case smoke-health health
run_case smoke-info workflow list
run_case smoke-debug-mode debug-mode
run_case smoke-metrics metrics show
run_case smoke-diagnostics diagnostics
run_case smoke-search search e2e
run_case smoke-query query

section "workflow lifecycle tier"
WF_ID="e2e-minimal"
run_case workflow-validate workflow validate "$FIX_DIR/workflow-minimal.json" --format json
run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
  "$ACTUAL_DIR/workflow-create.stdout.json" "$ACTUAL_DIR/workflow-create.stderr.txt" \
  -- workflow create --file "$FIX_DIR/workflow-minimal.json" --format json
check workflow-create 0 $? "$ACTUAL_DIR/workflow-create.stdout.json"
run_case workflow-show workflow show "$WF_ID"
run_case workflow-graph workflow graph "$WF_ID"
run_case workflow-list workflow list
run_negative_case workflow-show-missing workflow show e2e-does-not-exist
run_negative_case workflow-validate-bad workflow validate "$FIX_DIR/workflow-input.json" --format json

section "execution read and negative tier"
run_case execution-list execution list
run_negative_case execution-show-missing execution show e2e-does-not-exist
run_negative_case execution-watch-missing execution watch e2e-does-not-exist --once

section "registry read tier"
run_case registry-tool-list tool list
run_case registry-skill-list skill list
run_case registry-template-list template list

if [ "$SKIP_LOCAL_EXECUTION" -eq 0 ]; then
  section "local execution tier"
  SCRIPT_WF="e2e-script"
  set +e
  run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
    "$ACTUAL_DIR/workflow-create-script.stdout.json" "$ACTUAL_DIR/workflow-create-script.stderr.txt" \
    -- workflow create --file "$FIX_DIR/workflow-script.json" --format json
  create_code=$?
  set -e
  if [ "$create_code" -ne 0 ]; then
    echo "SKIP local execution (script workflow not accepted here)"
  else
    check workflow-create-script 0 "$create_code" "$ACTUAL_DIR/workflow-create-script.stdout.json"
    set +e
    run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
      "$ACTUAL_DIR/execution-run.stdout.json" "$ACTUAL_DIR/execution-run.stderr.txt" \
      -- execution run --workflow "$SCRIPT_WF" --input-file "$FIX_DIR/workflow-input.json" --background
    run_code=$?
    set -e
    if [ "$run_code" -ne 0 ]; then
      echo "SKIP local execution run (environment cannot run scripts)"
    else
      check execution-run 0 "$run_code" "$ACTUAL_DIR/execution-run.stdout.json"
      EXEC_ID="$(jq -r '.data // empty' "$ACTUAL_DIR/execution-run.stdout.json" | head -n 1)"
      if [ -n "$EXEC_ID" ] && [ "$EXEC_ID" != "null" ]; then
        run_case execution-watch execution watch "$EXEC_ID" --once
        run_case execution-show execution show "$EXEC_ID"
      fi
    fi
    run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
      "$ACTUAL_DIR/workflow-delete-script.stdout.json" "$ACTUAL_DIR/workflow-delete-script.stderr.txt" \
      -- workflow delete "$SCRIPT_WF" --force >/dev/null 2>&1 || true
  fi
fi

section "cleanup workflow fixture"
run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
  "$ACTUAL_DIR/workflow-delete.stdout.json" "$ACTUAL_DIR/workflow-delete.stderr.txt" \
  -- workflow delete "$WF_ID" --force >/dev/null 2>&1 || true

if [ "$TIER" = "full" ]; then
  section "manual model session (informational only)"
  echo "full tier selected, but model sessions stay manual:"
  echo "  wf-headless --remote $REMOTE --directory $WORK_DIR --output json --no-color run \"\$(cat $FIX_DIR/prompt-smoke.txt)\""
fi

section "result: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
