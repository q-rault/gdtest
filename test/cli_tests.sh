#!/usr/bin/env bash
# test/cli_tests.sh - shell-level integration tests for gdtest CLI behaviour.
#
# Verifies features that can only be observed at the process level:
# exit codes, TAP output, reporters, and `gdtest init` against a real
# temporary Godot project.
#
# Run from the gdtest repo root:
#   bash test/cli_tests.sh
#
# Prerequisites:
#   - cargo build --release already done
#   - GODOT_EXE set in environment or .env for tests that spawn real Godot

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -f "$REPO_ROOT/.env" ]]; then
    # shellcheck disable=SC1091
    source "$REPO_ROOT/.env"
fi

GDTEST="$REPO_ROOT/target/release/gdtest"
[[ -x "$GDTEST" ]] || { echo "ERROR: $GDTEST not found - run: cargo build --release" >&2; exit 1; }
REAL_GODOT_EXE="${GODOT_EXE:-}"

PASS=0
FAIL=0
SKIP=0

ok()   { echo "ok     - $1"; PASS=$(( PASS + 1 )); }
fail() { echo "FAIL   - $1"; FAIL=$(( FAIL + 1 )); }
skip() { echo "skip   - $1"; SKIP=$(( SKIP + 1 )); }

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

run_capture() {
    local __var_name="$1"; shift
    local __output
    if ! __output="$("$@" 2>&1)"; then
        printf -v "$__var_name" '%s' "$__output"
        return 1
    fi
    printf -v "$__var_name" '%s' "$__output"
    return 0
}

assert_file_exists() {
    local label="$1"; local path="$2"
    if [[ -f "$path" ]]; then
        ok "$label"
    else
        fail "$label"
    fi
}

assert_file_missing() {
    local label="$1"; local path="$2"
    if [[ ! -e "$path" ]]; then
        ok "$label"
    else
        fail "$label"
    fi
}

with_real_godot() {
    if [[ -z "$REAL_GODOT_EXE" ]]; then
        skip "$1"
        return 1
    fi
    return 0
}

make_temp_project() {
    local dir
    dir="$(mktemp -d "${TMPDIR:-/tmp}/gdtest-init-XXXXXX")"
    mkdir -p "$dir/test"
    cat > "$dir/project.godot" <<'EOF'
config_version=5

[application]
config/name="gdtest-e2e"
run/main_scene=""

[rendering]
renderer/rendering_method="gl_compatibility"
renderer/rendering_method.mobile="gl_compatibility"
EOF
    cat > "$dir/test/test_sample.gd" <<'EOF'
extends GdTest

func test_addition() -> void:
    assert_eq(1 + 1, 2)
EOF
    echo "$dir"
}

FIXTURES="$SCRIPT_DIR/fixtures"
TMP_DIRS=()
cleanup() {
    local dir
    for dir in "${TMP_DIRS[@]}"; do
        rm -rf "$dir"
    done
}
trap cleanup EXIT

echo "=== gdtest CLI integration tests ==="
echo ""

# --list
run_grep "--list prints test names" "test_normal_runs" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --list

# --filter
run_grep "--filter runs only matching test" "test_normal_runs" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --filter "normal" --reporter tap

# --fail-fast
run_exit_nonzero "--fail-fast exits non-zero on failure" \
    "$GDTEST" "$FIXTURES/fail_fast.gd" --fail-fast --reporter tap

if "$GDTEST" "$FIXTURES/fail_fast.gd" --fail-fast --reporter tap 2>&1 | grep -qE "ok.*test_second_would_pass"; then
    fail "--fail-fast must not run tests after the first failure"
else
    ok "--fail-fast stops after first failure"
fi

# --reporter junit
run_grep "--reporter junit produces XML" "<testsuites" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --reporter junit

run_grep "JUnit XML has testcase element" "<testcase" \
    "$GDTEST" "$FIXTURES/xtest_skip.gd" --reporter junit

# gdtest init (default installs runtime + editor)
TMP_PROJECT="$(make_temp_project)"
TMP_DIRS+=("$TMP_PROJECT")
run_exit_ok "gdtest init succeeds on a fresh project" \
    "$GDTEST" init --project-path "$TMP_PROJECT"

assert_file_exists "gdtest init installs runtime worker" \
    "$TMP_PROJECT/addons/gdtest/gdtest_worker.gd"
assert_file_exists "gdtest init installs editor plugin by default" \
    "$TMP_PROJECT/addons/gdtest_editor/plugin.cfg"

if with_real_godot "fresh project end-to-end tests require real Godot"; then
    run_grep "fresh project runs tests end-to-end after gdtest init via explicit --godot-exe" "ok .*test_addition" \
        "$GDTEST" --godot-exe "$REAL_GODOT_EXE" --project-path "$TMP_PROJECT" "$TMP_PROJECT/test" --reporter tap

    TMP_FAKE_PATH="$(mktemp -d "${TMPDIR:-/tmp}/gdtest-godot-path-XXXXXX")"
    TMP_DIRS+=("$TMP_FAKE_PATH")
    cat > "$TMP_FAKE_PATH/godot" <<EOF
#!/usr/bin/env bash
exec "$REAL_GODOT_EXE" "\$@"
EOF
    chmod +x "$TMP_FAKE_PATH/godot"
    run_grep "fresh project runs tests end-to-end through zero-config Godot discovery" "ok .*test_addition" \
        env -u GODOT_EXE PATH="$TMP_FAKE_PATH:$PATH" \
        "$GDTEST" --project-path "$TMP_PROJECT" "$TMP_PROJECT/test" --reporter tap
fi

# gdtest init --no-editor
TMP_NO_EDITOR="$(make_temp_project)"
TMP_DIRS+=("$TMP_NO_EDITOR")
run_exit_ok "gdtest init --no-editor succeeds on a fresh project" \
    "$GDTEST" init --project-path "$TMP_NO_EDITOR" --no-editor

assert_file_exists "gdtest init --no-editor still installs runtime" \
    "$TMP_NO_EDITOR/addons/gdtest/gdtest_worker.gd"
assert_file_missing "gdtest init --no-editor skips editor plugin" \
    "$TMP_NO_EDITOR/addons/gdtest_editor/plugin.cfg"

if with_real_godot "no-editor end-to-end tests require real Godot"; then
    run_grep "project initialized with --no-editor still runs tests end-to-end" "ok .*test_addition" \
        "$GDTEST" --godot-exe "$REAL_GODOT_EXE" --project-path "$TMP_NO_EDITOR" "$TMP_NO_EDITOR/test" --reporter tap
fi

# gdtest init overwrite protection
TMP_FORCE="$(make_temp_project)"
TMP_DIRS+=("$TMP_FORCE")
run_exit_ok "gdtest init can prepare a project before overwrite test" \
    "$GDTEST" init --project-path "$TMP_FORCE"

printf 'modified\n' > "$TMP_FORCE/addons/gdtest/gdt.gd"
if run_capture INIT_FORCE_OUT "$GDTEST" init --project-path "$TMP_FORCE"; then
    fail "gdtest init refuses to overwrite modified files without --force"
else
    if grep -q -- "--force" <<<"$INIT_FORCE_OUT"; then
        ok "gdtest init refuses to overwrite modified files without --force"
    else
        fail "gdtest init reports why overwrite was refused"
    fi
fi

run_exit_ok "gdtest init --force overwrites modified files" \
    "$GDTEST" init --project-path "$TMP_FORCE" --force
if with_real_godot "force-repair end-to-end tests require real Godot"; then
    run_grep "project repaired with --force runs tests end-to-end" "ok .*test_addition" \
        "$GDTEST" --godot-exe "$REAL_GODOT_EXE" --project-path "$TMP_FORCE" "$TMP_FORCE/test" --reporter tap
fi

# exit code
run_exit_nonzero "non-zero exit on test failure" \
    "$GDTEST" "$FIXTURES/fail_fast.gd" --reporter tap

echo ""
echo "-----------------------------------------"
echo "  passed: $PASS   failed: $FAIL   skipped: $SKIP"
echo "-----------------------------------------"

[[ $FAIL -eq 0 ]]
