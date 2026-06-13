#!/usr/bin/env bash
# benchmarks/eval_gut_game.sh -- repeatable evaluation harness for a GUT-based
# Godot project before migrating it to gdtest.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GDTEST_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -f "$GDTEST_ROOT/.env" ]]; then
    # shellcheck disable=SC1091
    source "$GDTEST_ROOT/.env"
fi

: "${GAME_DIR:?GAME_DIR must point at the Godot project to evaluate}"

GODOT_EXE="${GODOT_EXE:-godot4}"
GDTEST_BIN="${GDTEST_BIN:-$GDTEST_ROOT/target/release/gdtest}"
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/results/$(basename "$GAME_DIR")-gut-eval}"
TEST_DIR="${TEST_DIR:-$GAME_DIR/test}"
HARNESS_DEST="${HARNESS_DEST:-$GAME_DIR/addons/gdtest}"
HARNESS_PATH="${HARNESS_PATH:-addons/gdtest/gdtest_worker.gd}"
JOBS="${JOBS:-4}"
ISOLATION="${ISOLATION:-file}"

[[ -x "$GDTEST_BIN" ]] || {
    echo "missing $GDTEST_BIN - run cargo build --release or set GDTEST_BIN" >&2
    exit 1
}
[[ -d "$GAME_DIR" ]] || {
    echo "missing GAME_DIR: $GAME_DIR" >&2
    exit 1
}
[[ -d "$TEST_DIR" ]] || {
    echo "missing TEST_DIR: $TEST_DIR" >&2
    exit 1
}

if python3 -V >/dev/null 2>&1; then
    PYTHON=(python3)
elif py -3 -V >/dev/null 2>&1; then
    PYTHON=(py -3)
elif python -V >/dev/null 2>&1; then
    PYTHON=(python)
else
    echo "missing Python interpreter (tried python3, python, py -3)" >&2
    exit 1
fi

mkdir -p "$RESULTS_DIR"

now_ms() {
    "${PYTHON[@]}" -c 'import time; print(int(time.time() * 1000))'
}

write_kv() {
    local key="$1"
    local value="$2"
    printf '%s=%s\n' "$key" "$value" >> "$RESULTS_DIR/summary.env"
}

safe_ratio() {
    local left="$1"
    local right="$2"
    "${PYTHON[@]}" - "$left" "$right" <<'PY'
import sys
left = float(sys.argv[1])
right = float(sys.argv[2])
if right <= 0:
    print("n/a")
else:
    print(f"{left / right:.2f}x")
PY
}

copy_harness() {
    mkdir -p "$HARNESS_DEST"
    cp "$GDTEST_ROOT"/addons/gdtest/* "$HARNESS_DEST/"
}

count_matches() {
    local pattern="$1"
    local matches
    matches="$(rg -o "$pattern" "$TEST_DIR" -g "*.gd" || true)"
    if [[ -z "$matches" ]]; then
        echo "0"
    else
        printf '%s\n' "$matches" | wc -l | tr -d ' '
    fi
}

top_files_for() {
    local pattern="$1"
    rg -l "$pattern" "$TEST_DIR" -g "*.gd" | sort | head -10
}

candidate_block() {
    local title="$1"
    local pattern="$2"
    {
        echo "## $title"
        echo
        echo '```sh'
        printf "rg -l '%s' \"\$TEST_DIR\" -g '*.gd' | sort | head -10\n" "$pattern"
        echo '```'
        echo
        top_files_for "$pattern" || true
        echo
    } >> "$RESULTS_DIR/gut_porting_candidates.md"
}

echo "== Warm-up import =="
"$GODOT_EXE" --path "$GAME_DIR" --headless --import > "$RESULTS_DIR/import.txt" 2>&1 || true

echo "== Axis A: gdtest setup =="
SETUP_START=$(now_ms)
copy_harness
LIST_EXIT=0
"$GDTEST_BIN" "$TEST_DIR" \
    --godot-exe "$GODOT_EXE" \
    --project-path "$GAME_DIR" \
    --harness "$HARNESS_PATH" \
    --list > "$RESULTS_DIR/gdtest_list.txt" 2>&1 || LIST_EXIT=$?
SETUP_END=$(now_ms)
SETUP_MS=$((SETUP_END - SETUP_START))
printf '%s\n' "$SETUP_MS" > "$RESULTS_DIR/gdtest_setup_time_ms.txt"
: > "$RESULTS_DIR/summary.env"
write_kv "SETUP_MS" "$SETUP_MS"
write_kv "GDTEST_LIST_EXIT" "$LIST_EXIT"

echo "== Axis B: GUT API inventory =="
find "$TEST_DIR" -name "*.gd" | sort > "$RESULTS_DIR/gut_test_files.txt"
{
    echo "pattern,count"
    for pattern in \
        'assert_eq\b' \
        'assert_ne\b' \
        'assert_true\b' \
        'assert_false\b' \
        'assert_null\b' \
        'assert_not_null\b' \
        'assert_has\b' \
        'assert_does_not_have\b' \
        'assert_almost_eq\b' \
        'watch_signals\b' \
        'assert_signal_emitted\b' \
        'yield_to\b' \
        'wait_for_signal\b' \
        'double\s*\(' \
        'stub\s*\(' \
        'before_each\b' \
        'after_each\b' \
        'use_parameters\b'
    do
        printf '%s,%s\n' "$pattern" "$(count_matches "$pattern")"
    done
} > "$RESULTS_DIR/gut_assertions.csv"

{
    echo "# GUT porting candidates"
    echo
    echo "Generated from \`$TEST_DIR\`."
    echo
} > "$RESULTS_DIR/gut_porting_candidates.md"

candidate_block "Assertions simples" 'assert_eq\b'
candidate_block "Comparaison numerique" 'assert_almost_eq\b|assert_gt\b|assert_lt\b|assert_gte\b|assert_lte\b'
candidate_block "Collections" 'assert_has\b|assert_does_not_have\b'
candidate_block "Signaux" 'watch_signals\b|assert_signal_emitted\b|assert_signal_not_emitted\b'
candidate_block "Async / await" 'yield_to\b|wait_for_signal\b|await\b'
candidate_block "Mocks / doubles" 'double\s*\(|partial_double\s*\(|stub\s*\('
candidate_block "Lifecycle hooks" 'before_each\b|after_each\b|before_all\b|after_all\b'
candidate_block "Parametriques" 'use_parameters\b'

echo "== Axis E: baseline GUT =="
GUT_START=$(now_ms)
GUT_EXIT=0
"$GODOT_EXE" --path "$GAME_DIR" --headless \
    -s res://addons/gut/gut_cmdln.gd -- \
    -gdir="res://$(basename "$TEST_DIR")" -gexit \
    > "$RESULTS_DIR/gut_output.txt" 2>&1 || GUT_EXIT=$?
GUT_END=$(now_ms)
GUT_MS=$((GUT_END - GUT_START))
printf '%s\n' "$GUT_MS" > "$RESULTS_DIR/gut_time_ms.txt"
write_kv "GUT_MS" "$GUT_MS"
write_kv "GUT_EXIT" "$GUT_EXIT"

echo "== Axis E/F: gdtest run + JUnit =="
GDTEST_START=$(now_ms)
GDTEST_EXIT=0
"$GDTEST_BIN" "$TEST_DIR" \
    --godot-exe "$GODOT_EXE" \
    --project-path "$GAME_DIR" \
    --harness "$HARNESS_PATH" \
    --reporter junit \
    --isolation "$ISOLATION" \
    -j "$JOBS" \
    > "$RESULTS_DIR/gdtest_junit.xml" 2> "$RESULTS_DIR/gdtest_stderr.txt" || GDTEST_EXIT=$?
GDTEST_END=$(now_ms)
GDTEST_MS=$((GDTEST_END - GDTEST_START))
printf '%s\n' "$GDTEST_MS" > "$RESULTS_DIR/gdtest_time_ms.txt"
write_kv "GDTEST_MS" "$GDTEST_MS"
write_kv "GDTEST_EXIT" "$GDTEST_EXIT"
write_kv "JOBS" "$JOBS"
write_kv "ISOLATION" "$ISOLATION"

XML_STATUS="missing"
if command -v xmllint >/dev/null 2>&1; then
    if xmllint --noout "$RESULTS_DIR/gdtest_junit.xml" > "$RESULTS_DIR/xmllint.txt" 2>&1; then
        XML_STATUS="ok"
    else
        XML_STATUS="invalid"
    fi
else
    printf '%s\n' "xmllint not found" > "$RESULTS_DIR/xmllint.txt"
    XML_STATUS="skipped"
fi
write_kv "XML_STATUS" "$XML_STATUS"

RATIO=$(safe_ratio "$GUT_MS" "$GDTEST_MS")
write_kv "GUT_OVER_GDTEST" "$RATIO"

cat > "$RESULTS_DIR/README.md" <<EOF
# GUT evaluation results

- Game: \`$GAME_DIR\`
- Test dir: \`$TEST_DIR\`
- gdtest jobs: \`$JOBS\`
- gdtest isolation: \`$ISOLATION\`

## Timings

- gdtest setup: \`$SETUP_MS ms\`
- GUT baseline: \`$GUT_MS ms\`
- gdtest run: \`$GDTEST_MS ms\`
- GUT / gdtest ratio: \`$RATIO\`

## Exit codes

- \`gdtest --list\`: \`$LIST_EXIT\`
- GUT baseline: \`$GUT_EXIT\`
- gdtest JUnit run: \`$GDTEST_EXIT\`
- XML status: \`$XML_STATUS\`

## Files

- \`gdtest_list.txt\`
- \`gut_output.txt\`
- \`gdtest_junit.xml\`
- \`gdtest_stderr.txt\`
- \`gut_assertions.csv\`
- \`gut_porting_candidates.md\`
- \`summary.env\`
EOF

echo
echo "Results written to $RESULTS_DIR"
echo "Setup:   ${SETUP_MS} ms"
echo "GUT:     ${GUT_MS} ms"
echo "gdtest:  ${GDTEST_MS} ms"
echo "Ratio:   ${RATIO}"
echo "XML:     ${XML_STATUS}"
