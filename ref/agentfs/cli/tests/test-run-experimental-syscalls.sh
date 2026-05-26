#!/bin/sh
#
# Test syscalls through agentfs run --experimental-sandbox (ptrace-based).
#
# This tests AgentFS file operations using the ptrace-based sandbox
# where files are stored in the AgentFS database at /agent.
#
set -e

echo -n "TEST syscalls (agentfs run --experimental-sandbox)... "

DIR="$(dirname "$0")"

# Compile the test program
make -C "$DIR/syscall" clean > /dev/null 2>&1
make -C "$DIR/syscall" > /dev/null 2>&1

TEST_DB="agent.db"

# Clean up any existing test database
rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

# Initialize the database
cargo run -- init > /dev/null 2>&1

# Populate with test files using experimental sandbox
# The experimental sandbox mounts agent.db at /agent
cargo run -- run --experimental-sandbox /bin/bash -c 'echo "Hello from virtual FD!" > /agent/test.txt' > /dev/null 2>&1

# Create existing.txt for the append test
cargo run -- run --experimental-sandbox /bin/bash -c 'echo -n "original content" > /agent/existing.txt' > /dev/null 2>&1

# Note: The nested directory test (test_append_nested) is skipped for experimental sandbox
# because it tests FUSE overlay COW behavior where parent dirs need to be created in delta.
# The experimental sandbox has no base layer, so this scenario doesn't apply.

# Run the syscall tests using the experimental ptrace-based sandbox
if ! output=$(cargo run -- run --experimental-sandbox "$DIR/syscall/test-syscalls" /agent 2>&1); then
    echo "FAILED"
    echo "Output was: $output"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

echo "$output" | grep -q "All tests passed!" || {
    echo "FAILED: 'All tests passed!' not found"
    echo "Output was: $output"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
}

# Verify output file was created (by reading it back)
if ! output=$(cargo run -- run --experimental-sandbox /bin/cat /agent/output.txt 2>&1); then
    echo "FAILED: output.txt was not created or readable"
    rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"
    exit 1
fi

rm -f "$TEST_DB" "${TEST_DB}-wal" "${TEST_DB}-shm"

echo "OK"
