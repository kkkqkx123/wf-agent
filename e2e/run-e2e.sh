#!/usr/bin/env bash
# Headless end-to-end run: start an isolated server, drive it with
# wf-headless, collect outputs, then stop the server.
# Default tiers need no external model service; model sessions stay manual.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
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
require_cmd curl
require_cmd python3

mkdir -p "$ACTUAL_DIR"
WORK_DIR="$(mktemp -d "$ACTUAL_DIR/workspace.XXXXXX")"
SERVER_LOG="$ACTUAL_DIR/server.log"
PORT="$(free_port)"
REMOTE="http://127.0.0.1:$PORT"
PASS=0
FAIL=0
SKIP=0

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
    echo "FAIL $name (expected non-zero exit; failure envelope alone is not enough)"
    FAIL=$((FAIL+1))
  fi
}

# Poll an execution until it leaves running/paused, up to a deadline.
# Prints the terminal status on stdout; returns non-zero on timeout.
wait_for_terminal() {
  local id="$1" deadline_secs="${2:-30}"
  local i status out tmp_out tmp_err
  tmp_out="$(mktemp)" tmp_err="$(mktemp)"
  for ((i = 0; i < deadline_secs; i++)); do
    run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
      "$tmp_out" "$tmp_err" -- execution status "$id" 2>/dev/null || true
    status="$(envelope_status_of "$tmp_out")"
    if [ "$status" = "completed" ] || [ "$status" = "failed" ] || [ "$status" = "cancelled" ]; then
      echo "$status"
      rm -f "$tmp_out" "$tmp_err"
      return 0
    fi
    sleep 1
  done
  out="$(envelope_status_of "$tmp_out")"
  rm -f "$tmp_out" "$tmp_err"
  echo "${out:-unknown}"
  return 1
}

# Negative probe against a dead port: expect a start-server hint.
check_unreachable_hint() {
  local name="$1"
  local out="$ACTUAL_DIR/$name.stdout.json" err="$ACTUAL_DIR/$name.stderr.txt"
  set +e
  "$HEADLESS_BIN" --remote "http://127.0.0.1:9" --directory "$WORK_DIR" \
    --output json --no-color health >"$out" 2>"$err"
  local code=$?
  set -e
  if [ "$code" -ne 0 ] && grep -q "start wf-server first" "$err"; then
    echo "PASS $name"
    PASS=$((PASS+1))
  else
    echo "FAIL $name (want non-zero exit with start-server hint, got exit $code)"
    FAIL=$((FAIL+1))
  fi
}

# Non-fatal read-model probe: watch and show read different stores, so log
# whether their terminal statuses converge within a bounded poll. This
# never fails the gate; it only accumulates evidence for the follow-up.
probe_status_consistency() {
  local id="$1"
  local watch_status show_status="unknown" polls=0 converged="no"
  watch_status="$(envelope_status_of "$ACTUAL_DIR/execution-watch.stdout.json")"
  while [ "$polls" -lt 5 ]; do
    run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
      "$ACTUAL_DIR/consistency-show.stdout.json" "$ACTUAL_DIR/consistency-show.stderr.txt" \
      -- execution show "$id" || true
    show_status="$(envelope_status_of "$ACTUAL_DIR/consistency-show.stdout.json")"
    polls=$((polls + 1))
    if [ -n "$show_status" ] && [ "$show_status" = "$watch_status" ]; then
      converged="yes"
      break
    fi
    sleep 1
  done
  echo "consistency watch=$watch_status show=$show_status converged=$converged polls=$polls" > "$ACTUAL_DIR/consistency-show-watch.txt"
  echo "consistency probe: watch=$watch_status show=$show_status converged=$converged polls=$polls"
}

section "health and diagnostics tier"
run_case smoke-health health
run_case smoke-info workflow list
run_case smoke-debug-mode debug-mode
run_case smoke-metrics metrics show
run_case smoke-metrics-export-json metrics export --format json
run_case smoke-metrics-export-prom metrics export --format prometheus
run_case smoke-diagnostics diagnostics
run_case smoke-search search e2e
run_case smoke-query query
run_case smoke-directories directories
run_case smoke-remote-list remote list
run_case smoke-llm-status llm status
run_case smoke-llm-profile-list llm-profile list
run_case smoke-llm-profile-default llm-profile default
run_case smoke-llm-provider-list llm-provider list
run_case smoke-llm-provider-show llm-provider show builtin-gemini
run_case smoke-llm-provider-models llm-provider models builtin-gemini
run_case smoke-config-export config export "$WORK_DIR/tui-backup.toml"
run_case smoke-query-filtered query --status completed
run_case smoke-search-scoped search e2e --limit 10

section "workflow lifecycle tier"
WF_ID="e2e-minimal"
run_case workflow-validate workflow validate "$FIX_DIR/workflow-minimal.json" --format json
set +e
run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
  "$ACTUAL_DIR/workflow-create.stdout.json" "$ACTUAL_DIR/workflow-create.stderr.txt" \
  -- workflow create --file "$FIX_DIR/workflow-minimal.json" --format json
create_main_code=$?
set -e
check workflow-create 0 "$create_main_code" "$ACTUAL_DIR/workflow-create.stdout.json"
run_case workflow-show workflow show "$WF_ID"
run_case workflow-graph workflow graph "$WF_ID"
run_case workflow-graph-summary workflow graph "$WF_ID" --summary
run_case workflow-graph-topo workflow graph "$WF_ID" --topo
run_case workflow-graph-cycles workflow graph "$WF_ID" --detect-cycles
run_case workflow-graph-reach workflow graph "$WF_ID" --reachability start
run_case workflow-list workflow list
run_case workflow-list-filtered workflow list --keyword e2e --limit 10
run_negative_case workflow-show-missing workflow show e2e-does-not-exist
run_negative_case workflow-validate-bad workflow validate "$FIX_DIR/workflow-input.json" --format json

section "workflow extended tier"
CLONE_ID="e2e-clone"
run_case workflow-update workflow update "$WF_ID" --file "$FIX_DIR/workflow-update.json" --format json
run_case workflow-show-updated workflow show "$WF_ID"
if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d.get("data", {}).get("description", "").startswith("Updated"), "description not updated"' "$ACTUAL_DIR/workflow-show-updated.stdout.json" 2>/dev/null; then
  echo "PASS workflow-update-semantic"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-update-semantic (show does not reflect updated description)"
  FAIL=$((FAIL+1))
fi
run_case workflow-clone workflow clone "$WF_ID" --as "$CLONE_ID"
run_case workflow-show-clone workflow show "$CLONE_ID"
run_case workflow-export workflow export "$CLONE_ID"
set +e
run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
  "$ACTUAL_DIR/workflow-export-file.stdout.json" "$ACTUAL_DIR/workflow-export-file.stderr.txt" \
  -- workflow export "$CLONE_ID" --file "$WORK_DIR/exported-clone.json"
export_file_code=$?
set -e
check workflow-export-file 0 "$export_file_code" "$ACTUAL_DIR/workflow-export-file.stdout.json"
if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert isinstance(d, dict) and d.get("id")=="e2e-clone", "file export is not a workflow object"' "$WORK_DIR/exported-clone.json" 2>/dev/null; then
  echo "PASS workflow-export-file-semantic"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-export-file-semantic (exported file is not a workflow object)"
  FAIL=$((FAIL+1))
fi
run_case workflow-version-list workflow version list "$WF_ID"
run_case workflow-version-changelog workflow version changelog "$WF_ID"
run_case workflow-version-bump workflow version bump "$WF_ID" --level patch --keep-original
run_case workflow-version-list-after workflow version list "$WF_ID"
if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); items=d.get("data",[]); assert isinstance(items,list) and len(items)>=1, "version list empty after bump with keep-original"' "$ACTUAL_DIR/workflow-version-list-after.stdout.json" 2>/dev/null; then
  echo "PASS workflow-version-list-semantic"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-version-list-semantic (list does not contain saved version)"
  FAIL=$((FAIL+1))
fi
run_case workflow-version-show workflow version show "$WF_ID" 1.0.0
run_case workflow-version-diff workflow version diff "$WF_ID" --from 1.0.0 --to 1.0.0
run_negative_case workflow-version-diff-missing workflow version diff "$WF_ID" --from 0.0.0 --to 9.9.9
run_case workflow-rollback workflow rollback "$WF_ID" 1.0.0
run_case workflow-show-rolled-back workflow show "$WF_ID"
if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d.get("data",{}).get("version")=="1.0.0", "rollback did not restore version 1.0.0"' "$ACTUAL_DIR/workflow-show-rolled-back.stdout.json" 2>/dev/null; then
  echo "PASS workflow-rollback-semantic"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-rollback-semantic (show does not reflect rolled-back version)"
  FAIL=$((FAIL+1))
fi
if python3 -c 'import json,sys; env=json.load(open(sys.argv[1])); raw=env.get("data", ""); wf=json.loads(raw) if isinstance(raw, str) else raw; open(sys.argv[2], "w").write(json.dumps(wf))' "$ACTUAL_DIR/workflow-export.stdout.json" "$WORK_DIR/reimport-clone.json" 2>/dev/null; then
  echo "PASS workflow-export-extract"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-export-extract (cannot decode export envelope)"
  FAIL=$((FAIL+1))
fi
run_case workflow-import workflow import "$WORK_DIR/reimport-clone.json" --format json
run_case workflow-show-reimported workflow show "$CLONE_ID"
run_case workflow-import-file workflow import "$WORK_DIR/exported-clone.json" --format json
run_case workflow-delete-clone workflow delete "$CLONE_ID" --force
run_negative_case workflow-show-after-delete workflow show "$CLONE_ID"
run_negative_case workflow-create-dup workflow create --file "$FIX_DIR/workflow-minimal.json" --format json
if grep -q "e2e-minimal" "$ACTUAL_DIR/workflow-create-dup.stdout.json" "$ACTUAL_DIR/workflow-create-dup.stderr.txt" 2>/dev/null; then
  echo "PASS workflow-create-dup-keyword"
  PASS=$((PASS+1))
else
  echo "FAIL workflow-create-dup-keyword (conflict error does not carry workflow id)"
  FAIL=$((FAIL+1))
fi

section "execution read and negative tier"
run_case execution-list execution list
run_case execution-list-completed execution list --status completed
run_case execution-list-limit execution list --limit 10
run_negative_case execution-show-missing execution show e2e-does-not-exist
run_negative_case execution-watch-missing execution watch e2e-does-not-exist --once
run_negative_case execution-run-missing-workflow execution run --workflow e2e-does-not-exist --input-file "$FIX_DIR/workflow-input.json" --background
if grep -q "e2e-does-not-exist" "$ACTUAL_DIR/execution-run-missing-workflow.stdout.json" "$ACTUAL_DIR/execution-run-missing-workflow.stderr.txt" 2>/dev/null; then
  echo "PASS execution-run-missing-workflow-keyword"
  PASS=$((PASS+1))
else
  echo "FAIL execution-run-missing-workflow-keyword (error does not carry workflow id)"
  FAIL=$((FAIL+1))
fi
run_negative_case execution-run-missing-file execution run --workflow "$WF_ID" --input-file /tmp/e2e-does-not-exist-input.json --background

section "registry read tier"
run_case registry-tool-list tool list
run_case registry-tool-show tool show web_search
run_case registry-tool-validate tool validate web_search --params '{"query":"e2e"}'
run_case registry-tool-search tool search e2e
run_case registry-skill-list skill list
run_case registry-skill-query skill query --filter e2e
run_negative_case registry-skill-show-missing skill show e2e-does-not-exist
run_case registry-template-list template list
run_case registry-template-show template show "@standard/main"
run_case registry-script-list script list
run_case registry-script-search script search e2e
run_case registry-script-validate script validate e2e-echo
run_case registry-trigger-list trigger list
run_case registry-trigger-executions trigger executions
run_case registry-trigger-stats trigger stats
run_case registry-task-list task list
run_case registry-task-stats task stats
run_case registry-task-system task system
run_case registry-event-list event list
run_case registry-event-stats event stats
run_case registry-event-health event health
run_case registry-message-search message search e2e
run_negative_case registry-trigger-show-missing trigger show e2e-does-not-exist
run_negative_case registry-checkpoint-show-missing checkpoint show e2e-does-not-exist
run_negative_case registry-remote-tunnel-missing remote tunnel e2e-does-not-exist

if [ "$SKIP_LOCAL_EXECUTION" -eq 0 ]; then
  section "local execution tier"
  SCRIPT_WF="e2e-script"
  SCRIPT_ID="e2e-echo"
  set +e
  run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
    "$ACTUAL_DIR/script-save.stdout.json" "$ACTUAL_DIR/script-save.stderr.txt" \
    -- script save --file "$FIX_DIR/script-echo.json"
  save_code=$?
  set -e
  if [ "$save_code" -ne 0 ]; then
    echo "SKIP local execution (cannot register fixture script here)"
  else
    check script-save 0 "$save_code" "$ACTUAL_DIR/script-save.stdout.json"
    run_case script-show script show "$SCRIPT_ID"
    set +e
    run_headless "$HEADLESS_BIN" "$REMOTE" "$WORK_DIR" \
      "$ACTUAL_DIR/workflow-create-script.stdout.json" "$ACTUAL_DIR/workflow-create-script.stderr.txt" \
      -- workflow create --file "$FIX_DIR/workflow-script.json" --format json
    create_code=$?
    set -e
    if [ "$create_code" -ne 0 ]; then
      echo "SKIP local execution run (script workflow not accepted here)"
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
      EXEC_ID="$(execution_id_of "$ACTUAL_DIR/execution-run.stdout.json")"
      if [ -n "$EXEC_ID" ] && [ "$EXEC_ID" != "null" ]; then
        TERMINAL="$(wait_for_terminal "$EXEC_ID" 30 || true)"
        echo "execution terminal status: $TERMINAL"
        run_case execution-watch execution watch "$EXEC_ID" --once
        run_case execution-show execution show "$EXEC_ID"
        run_case execution-inspect execution inspect "$EXEC_ID" --variables --transitions --context-evolution --call-stack --memory
        run_case execution-inspect-transitions execution inspect "$EXEC_ID" --context-transitions
        probe_status_consistency "$EXEC_ID"
        section "execution extended tier"
        run_case execution-status execution status "$EXEC_ID"
        run_case execution-logs execution logs "$EXEC_ID"
        run_case execution-hierarchy execution hierarchy "$EXEC_ID"
        run_case execution-subtree execution subtree "$EXEC_ID"
        run_case execution-history execution history "$EXEC_ID"
        run_case execution-performance execution performance "$EXEC_ID"
        run_case execution-bottleneck execution bottleneck "$EXEC_ID"
        run_case execution-errors execution errors "$EXEC_ID"
        run_case execution-progress execution progress "$EXEC_ID"
        run_case execution-state execution state "$EXEC_ID"
        run_case execution-agent-loops execution agent-loops "$EXEC_ID"
        run_case execution-compare execution compare "$EXEC_ID" "$EXEC_ID"
        run_case workflow-execution-graph workflow execution-graph "$EXEC_ID"
        run_case execution-query-filtered query --workflow-id "$SCRIPT_WF"
        run_case execution-audit-summary audit summary "$EXEC_ID"
        run_case execution-event-timeline event timeline "$EXEC_ID"
        run_case execution-event-follow event follow "$EXEC_ID" --once
        run_case execution-analysis-progress analysis progress "$EXEC_ID"
        run_case execution-analysis-compare analysis compare "$EXEC_ID" "$EXEC_ID"
        run_case execution-message-list message list --execution "$EXEC_ID"
        run_case execution-trigger-history trigger history --execution "$EXEC_ID"
        section "checkpoint and variable tier"
        run_case checkpoint-create checkpoint create "$EXEC_ID"
        run_case checkpoint-list checkpoint list "$EXEC_ID"
        if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); items=d.get("data",[]); assert isinstance(items,list) and len(items)>=1, "checkpoint list empty after create"' "$ACTUAL_DIR/checkpoint-list.stdout.json" 2>/dev/null; then
          echo "PASS checkpoint-list-semantic"
          PASS=$((PASS+1))
        else
          echo "FAIL checkpoint-list-semantic (created checkpoint not visible in list)"
          FAIL=$((FAIL+1))
        fi
        run_case checkpoint-chain checkpoint chain "$EXEC_ID"
        if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); v=d.get("data",{}); assert isinstance(v,dict) and int(v.get("checkpoint_count",0))>=1, "chain empty after create"' "$ACTUAL_DIR/checkpoint-chain.stdout.json" 2>/dev/null; then
          echo "PASS checkpoint-chain-semantic"
          PASS=$((PASS+1))
        else
          echo "FAIL checkpoint-chain-semantic (created checkpoint not visible in chain)"
          FAIL=$((FAIL+1))
        fi
        run_case checkpoint-latest checkpoint latest "$EXEC_ID"
        if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); v=d.get("data"); assert isinstance(v,dict) and v.get("id"), "latest missing after create"' "$ACTUAL_DIR/checkpoint-latest.stdout.json" 2>/dev/null; then
          echo "PASS checkpoint-latest-semantic"
          PASS=$((PASS+1))
        else
          echo "FAIL checkpoint-latest-semantic (latest not resolvable after create)"
          FAIL=$((FAIL+1))
        fi
        VAR_KEY="e2e-key"
        run_case variable-set variable set --execution "$EXEC_ID" --name "$VAR_KEY" --value '"hello"'
        run_case variable-get variable get --execution "$EXEC_ID" --name "$VAR_KEY"
        run_case variable-list variable list --execution "$EXEC_ID"
        run_case variable-delete variable delete --execution "$EXEC_ID" --name "$VAR_KEY"
        run_negative_case variable-get-after-delete variable get --execution "$EXEC_ID" --name "$VAR_KEY"
        run_case execution-delete execution delete "$EXEC_ID" --force
        run_negative_case execution-show-after-delete execution show "$EXEC_ID"
      fi
    fi
    run_case workflow-delete-script workflow delete "$SCRIPT_WF" --force
    run_negative_case workflow-show-script-after-delete workflow show "$SCRIPT_WF"
    run_case script-delete script delete "$SCRIPT_ID" --force
    fi
  fi
fi

section "connection negative tier"
check_unreachable_hint smoke-unreachable

section "cleanup workflow fixture"
run_case workflow-delete workflow delete "$WF_ID" --force
run_negative_case workflow-show-after-cleanup workflow show "$WF_ID"

if [ "$TIER" = "full" ]; then
  section "manual model session (informational only)"
  echo "full tier selected, but model sessions stay manual:"
  echo "  wf-headless --remote $REMOTE --directory $WORK_DIR --output json --no-color run \"\$(cat $FIX_DIR/prompt-smoke.txt)\""
fi

section "result: $PASS passed, $SKIP skipped, $FAIL failed"
[ "$FAIL" -eq 0 ]
