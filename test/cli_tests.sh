#!/usr/bin/env bash
# test/cli_tests.sh — shell-level integration tests for gdtest CLI behaviour.
#
# Verifies features that can only be observed at the process level:
# exit codes, TAP output, --fail-fast, --list, --filter, --reporter junit, --retries.
#
# Run from the gdtest repo root:
#   bash test/cli_tests.sh
#
# Prerequisites:
#   - cargo build --release already done
#   - GODOT_EXE set in environment or .env

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -f "$REPO_ROOT/.env" ]]; then
    # shellcheck disable=SC1091
    source "$REPO_ROOT/.env"
fi

GDTEST="$REPO_ROOT/target/release/gdtest"
[[ -x "$GDTEST" ]] || { echo "ERROR: $GDTEST not found — run: cargo build --release" >&2; exit 1; }
: "${GODOT_EXE:?GODOT_EXE must be set in .env or environment}"

PASS=0
FAIL=0
SKIP=0

ok()   { echo "ok     — $1"; PASS=$(( PASS + 1 )); }
fail() { echo "FAIL   — $1"; FAIL=$(( FAIL + 1 )); }
skip() { echo "skip   — $1"; SKIP=$(( SKIP + 1 )); }

run_grep() {
    local label="$1"; local pattern="$2"; shift 2
    if "$@" 2>&1 | grep -qE "$pattern"; then
        ok "$label"
    else
        fail "$label"
    fi
}

run_exit_ok() {
    local label="$1"; shift
    if "$@" > /dev/null 2>&1; then
        ok "$label"
    else
        fail "$label"
    fi
}

run_exit_nonzero() {
    local label="$1"; shift
    if ! "$@" > /dev/null 2>&1; then
        ok "$label"
    else
        fail "$label"
    fi
}

FIXTURES="$SCRIPT_DIR/fixtures"

echo "=== gdtest CLI integration tests ==="
echo ""

# ── --list ────────────────────────────────────────────────────────────────────
run_grep "--list prints test names" "test_normal_runs" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --list

# ── --filter ──────────────────────────────────────────────────────────────────
run_grep "--filter runs only matching test" "test_normal_runs" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --filter "normal" --reporter tap

# ── --fail-fast ───────────────────────────────────────────────────────────────
run_exit_nonzero "--fail-fast exits non-zero on failure" \
    "$GDTEST" "$FIXTURES/fail_fast.gd" --fail-fast --reporter tap

# With fail-fast the second test should not appear as passed
if "$GDTEST" "$FIXTURES/fail_fast.gd" --fail-fast --reporter tap 2>&1 | grep -qE "ok.*test_second_would_pass"; then
    fail "--fail-fast must not run tests after the first failure"
else
    ok "--fail-fast stops after first failure"
fi

# ── --reporter junit ──────────────────────────────────────────────────────────
run_grep "--reporter junit produces XML" "<testsuites" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --reporter junit

run_grep "JUnit XML has testcase element" "<testcase" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --reporter junit

# ── exit code ─────────────────────────────────────────────────────────────────
run_exit_nonzero "non-zero exit on test failure" \
    "$GDTEST" "$FIXTURES/fail_fast.gd" --reporter tap

# ── summary ───────────────────────────────────────────────────────────────────
echo ""
echo "─────────────────────────────────────────"
echo "  passed: $PASS   failed: $FAIL   skipped: $SKIP"
echo "─────────────────────────────────────────"

[[ $FAIL -eq 0 ]]
