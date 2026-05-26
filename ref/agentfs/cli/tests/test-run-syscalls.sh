#!/bin/sh
#
# Test syscalls through agentfs run (FUSE overlay).
#
# This tests the copy-on-write behavior when files exist in the base layer
# (host filesystem) and are modified through the overlay.
#
# Requires user namespaces support.
#
set -e

echo -n "TEST syscalls (agentfs run - FUSE overlay)... "

DIR="$(dirname "$0")"

# Compile the test program
make -C "$DIR/syscall" clean > /dev/null 2>&1
make -C "$DIR/syscall" > /dev/null 2>&1

TEST_DB="agent.db"

# Clean up any existing test database
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database
cargo run -- init > /dev/null 2>&1

# Create pre-existing files in the BASE LAYER (current directory)
# These will trigger copy-on-write when modified through the overlay
echo -n "original content" > existing.txt
echo "Hello from test setup!" > test.txt

# Create nested directory structure for COW parent dir test
# This catches the bug where .git/logs/HEAD fails because parent dirs
# don't exist in delta layer during copy-on-write
mkdir -p subdir
echo -n "nested content" > subdir/nested.txt

# Run syscall tests through FUSE overlay
# The test binary runs inside the overlay where:
# - Files from current directory are visible (base layer)
# - Modifications go to the delta layer (AgentFS database)
# - O_APPEND on existing.txt triggers copy-on-write
if ! output=$(cargo run -- run "$DIR/syscall/test-syscalls" . 2>&1); then
    echo "FAILED"
    echo "Output was: $output"
    rm -rf "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm" existing.txt test.txt subdir
    exit 1
fi

echo "$output" | grep -q "All tests passed!" || {
    echo "FAILED: 'All tests passed!' not found"
    echo "Output was: $output"
    rm -rf "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm" existing.txt test.txt subdir
    exit 1
}

# Note: output.txt is created in the delta layer (session-specific) so we can't
# verify it with a separate agentfs run. The "All tests passed!" check is sufficient.

rm -rf "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm" existing.txt test.txt subdir

echo "OK"
