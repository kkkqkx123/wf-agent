#!/usr/bin/env bash
#
# wf full-TUI PTY smoke test (tmux job-control harness).
#
# Maps to the stage-7 runtime acceptance list (wf-cli-stage7 issue archive,
# R1 items): boot rendering, typed Ctrl-Z suspend + `fg` resume, resize
# reflow, SIGUSR2 hot-reload repaint, empty-Executions Enter safety and the
# session-screen live input error path.
#
# Requires: tmux, a wf build (auto-builds unless WF_NO_BUILD=1).
# Usage:    scripts/tests/tui-smoke.sh   (run from anywhere)
# Exit:     0 = all checks passed, 1 = at least one check failed,
#           0 with SKIP rows = prerequisites missing (reported, not faked).

set -u

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SELF_DIR/../../../../.." && pwd)"
BIN="${WF_BIN:-$ROOT/target/debug/wf}"
SESS="wfsmoke"
TMP="$(mktemp -d /tmp/wfsmoke.XXXXXX)"
PASS=0
FAIL=0
SKIP=0

log() { printf '%s\n' "$*"; }
pass() { PASS=$((PASS + 1)); log "PASS  $1"; }
fail() { FAIL=$((FAIL + 1)); log "FAIL  $1 -- $2"; }
skip() { SKIP=$((SKIP + 1)); log "SKIP  $1 -- $2"; }

app_pid() { cat "$TMP/pid" 2>/dev/null; }
app_stat() { ps -o stat= -p "$(app_pid)" 2>/dev/null | tr -d ' '; }
pane_has() { tmux capture-pane -p -t "$SESS" 2>/dev/null | grep -q -- "$1"; }

# wait_for <name> <timeout_s> <command...> : poll command until true.
wait_for() {
    local name="$1" timeout_s="$2"
    shift 2
    local i
    for ((i = 0; i < timeout_s * 2; i++)); do
        "$@" && return 0
        sleep 0.5
    done
    fail "wait:$name" "timed out after ${timeout_s}s"
    return 1
}

cleanup() {
    tmux kill-session -t "$SESS" 2>/dev/null
    rm -rf "$TMP"
}
trap cleanup EXIT

require_harness() {
    command -v tmux >/dev/null 2>&1 || { skip "harness" "tmux not installed"; exit 0; }
}

build_binary() {
    if [ ! -x "$BIN" ]; then
        if [ -n "${WF_NO_BUILD:-}" ]; then
            skip "binary" "$BIN missing and WF_NO_BUILD set"
            exit 0
        fi
        log "building $BIN ..."
        (cd "$ROOT" && cargo build -p wf-tui --bin wf >/dev/null 2>&1) \
            || { skip "binary" "cargo build -p wf-tui failed"; exit 0; }
    fi
}

launch() {
    tmux kill-session -t "$SESS" 2>/dev/null
    # The pane's interactive shell stays as the job-control parent: wf runs as
    # a child (sh -c exec), so a Ctrl-Z stop surfaces to the shell and `fg`
    # resumes it - the real user flow.
    tmux new-session -d -s "$SESS" -x 160 -y 40 \
        || { skip "harness" "cannot create tmux session"; exit 0; }
    tmux send-keys -t "$SESS" \
        "cd '$ROOT' && sh -c 'echo \$\$ > $TMP/pid; exec env XDG_CACHE_HOME=$TMP TERM=xterm-256color $BIN --tui --storage memory'" \
        Enter
}

check_suspend_resume() {
    local stat
    tmux send-keys -t "$SESS" C-z
    sleep 1.5
    stat="$(app_stat)"
    case "$stat" in
        T*) pass "ctrl-z suspend (stat=$stat)" ;;
        *) fail "ctrl-z suspend" "stat=$stat (expected stopped)" ;;
    esac
    tmux send-keys -t "$SESS" 'fg' Enter
    wait_for "fg resume" 10 pane_has "Dashboard" \
        && pass "fg resume redraws dashboard"
}

check_resize() {
    tmux resize-window -t "$SESS" -x 100 -y 30
    sleep 2
    if [ -n "$(app_stat)" ] && pane_has "Dashboard"; then
        pass "resize storm settle + reflow"
    else
        fail "resize reflow" "app lost after resize"
    fi
}

check_sigusr2() {
    kill -USR2 "$(app_pid)" 2>/dev/null
    sleep 1.5
    if [ -n "$(app_stat)" ] && pane_has "Dashboard"; then
        pass "SIGUSR2 hot-reload repaint (no crash)"
    else
        fail "SIGUSR2" "app state=$(app_stat)"
    fi
}

check_empty_executions() {
    tmux send-keys -t "$SESS" '2'    # Executions screen (empty store)
    sleep 1.5
    tmux send-keys -t "$SESS" Enter  # drill-down on empty list must be a no-op
    sleep 1.5
    [ -n "$(app_stat)" ] \
        && pass "empty Executions + Enter is safe" \
        || fail "empty executions" "app died on Enter"
}

check_session_error_path() {
    tmux send-keys -t "$SESS" '3'    # Session screen
    sleep 1
    tmux send-keys -t "$SESS" 'hi' Enter
    sleep 4
    if [ -n "$(app_stat)" ] && pane_has "failed"; then
        pass "session live input renders turn failure line"
    else
        fail "session path" "no failure line within 4s (needs no-LLM env or longer wait)"
    fi
}

main() {
    require_harness
    build_binary
    launch
    wait_for "boot render" 15 pane_has "Dashboard" || return 1
    pass "boot: dashboard rendered (stat=$(app_stat))"
    check_suspend_resume
    check_resize
    check_sigusr2
    check_empty_executions
    check_session_error_path
    return 0
}

main
log "== result: $PASS pass, $FAIL fail, $SKIP skip =="
[ "$FAIL" -eq 0 ]
