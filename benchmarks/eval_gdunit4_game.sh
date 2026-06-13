#!/usr/bin/env bash
# benchmarks/eval_gdunit4_game.sh -- repeatable evaluation harness for a
# gdUnit4-based Godot project before migrating it to gdtest.

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
RESULTS_DIR="${RESULTS_DIR:-$SCRIPT_DIR/results/$(basename "$GAME_DIR")-gdunit4-eval}"
TEST_DIR="${TEST_DIR:-$GAME_DIR/test}"
HARNESS_DEST="${HARNESS_DEST:-$GAME_DIR/addons/gdtest}"
HARNESS_PATH="${HARNESS_PATH:-addons/gdtest/gdtest_worker.gd}"
JOBS="${JOBS:-4}"
ISOLATION="${ISOLATION:-file}"
STAGED_TEST_ROOT="${STAGED_TEST_ROOT:-$GAME_DIR/.gdtest_eval}"
STAGED_TEST_DIR="${STAGED_TEST_DIR:-$STAGED_TEST_ROOT/$(basename "$TEST_DIR")}"
BASELINE_REPORT_DIR="${BASELINE_REPORT_DIR:-$RESULTS_DIR/baseline_gdunit4_report}"

[[ -x "$GDTEST_BIN" ]] || {
    echo "missing $GDTEST_BIN - run cargo build --release or set GDTEST_BIN" >&2
    exit 1
}
[[ -f "$GODOT_EXE" || "$GODOT_EXE" == godot4 || "$GODOT_EXE" == godot ]] || {
    echo "missing GODOT_EXE: $GODOT_EXE" >&2
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

normalize_fs_path() {
    local value="$1"
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -u "$value" 2>/dev/null || printf '%s\n' "$value"
    else
        printf '%s\n' "$value"
    fi
}

GAME_DIR="$(normalize_fs_path "$GAME_DIR")"
TEST_DIR="$(normalize_fs_path "$TEST_DIR")"
HARNESS_DEST="$(normalize_fs_path "$HARNESS_DEST")"
STAGED_TEST_ROOT="$(normalize_fs_path "$STAGED_TEST_ROOT")"
STAGED_TEST_DIR="$(normalize_fs_path "$STAGED_TEST_DIR")"
RESULTS_DIR="$(normalize_fs_path "$RESULTS_DIR")"
GDTEST_BIN="$(normalize_fs_path "$GDTEST_BIN")"
BASELINE_REPORT_DIR="$(normalize_fs_path "$BASELINE_REPORT_DIR")"
if [[ -f "$GODOT_EXE" || "$GODOT_EXE" == *:* ]]; then
    GODOT_EXE="$(normalize_fs_path "$GODOT_EXE")"
fi

mkdir -p "$RESULTS_DIR"
: > "$RESULTS_DIR/summary.env"

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

ensure_under_game_dir() {
    local label="$1"
    local target="$2"
    "${PYTHON[@]}" - "$label" "$GAME_DIR" "$target" <<'PY'
import os
import sys

label, game_dir, target = sys.argv[1:]
game_dir = os.path.realpath(game_dir)
target = os.path.realpath(target)
try:
    common = os.path.commonpath([game_dir, target])
except ValueError:
    common = ""
if common != game_dir:
    print(f"{label} must stay inside GAME_DIR: {target}", file=sys.stderr)
    sys.exit(1)
PY
}

abs_to_res() {
    "${PYTHON[@]}" - "$GAME_DIR" "$1" <<'PY'
import os
import sys

game_dir = os.path.realpath(sys.argv[1])
target = os.path.realpath(sys.argv[2])
common = os.path.commonpath([game_dir, target])
if common != game_dir:
    print(target)
else:
    rel = os.path.relpath(target, game_dir).replace("\\", "/")
    print(f"res://{rel}")
PY
}

validate_paths() {
    ensure_under_game_dir "HARNESS_DEST" "$HARNESS_DEST"
    ensure_under_game_dir "STAGED_TEST_DIR" "$STAGED_TEST_DIR"
}

discover_suites() {
    rg -l '^[[:space:]]*extends[[:space:]]+GdUnitTestSuite\b' "$TEST_DIR" -g "*.gd" | sort > "$RESULTS_DIR/gdunit4_suites.txt" || true
    if [[ ! -s "$RESULTS_DIR/gdunit4_suites.txt" ]]; then
        echo "no gdUnit4 suites found under $TEST_DIR" >&2
        write_kv "SUITE_COUNT" "0"
        exit 1
    fi
    local suite_count
    suite_count="$(wc -l < "$RESULTS_DIR/gdunit4_suites.txt" | tr -d ' ')"
    write_kv "SUITE_COUNT" "$suite_count"
}

build_inventory() {
    "${PYTHON[@]}" - "$TEST_DIR" "$RESULTS_DIR/gdunit4_suites.txt" "$RESULTS_DIR/assertion_stats.txt" "$RESULTS_DIR/gdunit4_porting_candidates.md" <<'PY'
import pathlib
import re
import sys

test_dir = pathlib.Path(sys.argv[1]).resolve()
suites_file = pathlib.Path(sys.argv[2])
stats_path = pathlib.Path(sys.argv[3])
candidates_path = pathlib.Path(sys.argv[4])

suites = [pathlib.Path(line.strip()) for line in suites_file.read_text(encoding="utf-8").splitlines() if line.strip()]

pattern_specs = [
    ("typed_assertions", "Typed/fluent assertions", r"\b(assert_that|assert_str|assert_int|assert_float|assert_bool|assert_array|assert_dict|assert_signal|assert_file)\s*\(", "direct"),
    ("flat_assertions", "Flat assertions", r"\b(assert_eq|assert_ne|assert_true|assert_false|assert_null|assert_not_null|assert_gt|assert_gte|assert_lt|assert_lte|assert_almost_eq)\b", "direct"),
    ("lifecycle_hooks", "Lifecycle hooks", r"\b(before|after|before_test|after_test)\s*\(", "direct"),
    ("signals", "Signals", r"\b(assert_signal|assert_signal_count|monitor_signals|verify_signal|await_millis|await_idle_frame|await_signal)\b", "partial"),
    ("scene_runner", "SceneRunner and scene helpers", r"\b(scene_runner|simulate_frames|simulate_action_|simulate_mouse_|simulate_key_)\b", "partial"),
    ("input_factory", "InputFactory.*", r"\bInputFactory\.", "blocking"),
    ("mocks", "Mocks, spies, verify()", r"\b(mock|spy|partial_mock|verify|verify_no_interactions|verify_no_more_interactions|reset)\s*\(", "partial"),
    ("deep_stubs", "Deep stubs", r"\bRETURN_DEEP_STUB\b|\bmock\s*\([^)]*RETURN_DEEP_STUB", "blocking"),
    ("param_annotations", "@GdUnitTestParameters", r"@GdUnitTestParameters\b", "blocking"),
    ("gdunit_tuple", "GdUnitTuple / tuple()", r"\bGdUnitTuple\b|\btuple\s*\(", "blocking"),
    ("annotation_mock", "@GdUnitMock", r"@GdUnitMock\b", "blocking"),
    ("fuzzing", "Fuzzer annotations", r"@GdUnitFuzzer\b|\bfuzz_[A-Za-z0-9_]*\s*\(", "partial"),
    ("temp_helpers", "Temp dir / file helpers", r"\b(create_temp_dir|clean_temp_dir|delete_temp_dir|assert_file)\b", "partial"),
]

axis_tags = [
    ("A", r"."),
    ("B", r"assert_|verify|mock|spy|scene_runner|InputFactory|GdUnitTuple|@GdUnitTestParameters"),
    ("C", r"@GdUnitTestParameters|GdUnitTuple|InputFactory|RETURN_DEEP_STUB|monitor_signals|verify_no_more_interactions|clean_temp_dir"),
    ("D", r"\b(assert_that|assert_signal|assert_file|verify)"),
    ("E", r"\b(test_|assert_)"),
    ("F", r"\b(assert_signal|verify|scene_runner|mock|spy)"),
    ("G", r"mock|spy|scene_runner|InputFactory|assert_signal|@GdUnitTestParameters|GdUnitTuple|create_temp_dir|clean_temp_dir"),
]

stats_rows = []
analysis_rows = []

for suite in suites:
    text = suite.read_text(encoding="utf-8", errors="replace")
    rel = suite.relative_to(test_dir).as_posix()
    test_count = len(re.findall(r"(?m)^\s*func\s+test_[A-Za-z0-9_]+\s*\(", text))
    per_file_counts = {}
    file_status = "portable"
    matched_labels = []
    for key, label, pattern, severity in pattern_specs:
        matches = re.findall(pattern, text)
        count = len(matches)
        per_file_counts[key] = count
        if count:
            matched_labels.append(label)
            if severity == "blocking":
                file_status = "exclude"
            elif severity == "partial" and file_status != "exclude":
                file_status = "portable-with-caveats"
    feature_score = sum(min(per_file_counts[key], 5) for key in per_file_counts)
    tags = [name for name, pattern in axis_tags if re.search(pattern, text)]
    analysis_rows.append({
        "path": rel,
        "test_count": test_count,
        "status": file_status,
        "score": feature_score,
        "features": matched_labels,
        "tags": tags,
        "counts": per_file_counts,
    })

for key, label, pattern, severity in pattern_specs:
    total = 0
    files = []
    for row in analysis_rows:
        count = row["counts"][key]
        total += count
        if count:
            files.append((row["path"], count))
    stats_rows.append((key, label, severity, total, files))

with stats_path.open("w", encoding="utf-8", newline="\n") as fh:
    fh.write("# gdUnit4 API/assertion inventory\n\n")
    fh.write("kind\tseverity\tcount\tfiles\n")
    for _key, label, severity, total, files in stats_rows:
        fh.write(f"{label}\t{severity}\t{total}\t{len(files)}\n")
    fh.write("\n# Top files per pattern\n")
    for _key, label, severity, total, files in stats_rows:
        fh.write(f"\n## {label} ({severity}, total={total})\n")
        if not files:
            fh.write("- none\n")
            continue
        for path, count in sorted(files, key=lambda item: (-item[1], item[0]))[:10]:
            fh.write(f"- {path} ({count})\n")

portable = [row for row in analysis_rows if row["status"] == "portable"]
portable_with_caveats = [row for row in analysis_rows if row["status"] == "portable-with-caveats"]
excluded = [row for row in analysis_rows if row["status"] == "exclude"]

ranked = sorted(
    portable + portable_with_caveats,
    key=lambda row: (-row["score"], -row["test_count"], row["path"]),
)
selected = []
cumulative = 0
for row in ranked:
    if len(selected) >= 12 and cumulative >= 20:
        break
    selected.append(row)
    cumulative += row["test_count"]
    if cumulative >= 30:
        break

with candidates_path.open("w", encoding="utf-8", newline="\n") as fh:
    fh.write("# gdUnit4 migration candidates\n\n")
    fh.write(f"Generated from `{test_dir.as_posix()}`.\n\n")
    fh.write("## Blocking patterns tracked explicitly\n\n")
    fh.write("- `@GdUnitTestParameters`\n")
    fh.write("- `GdUnitTuple` / `tuple(...)`\n")
    fh.write("- `InputFactory.*`\n")
    fh.write("- deep stubs (`RETURN_DEEP_STUB`)\n")
    fh.write("- temp/file helper drift (`clean_temp_dir`, related filesystem helpers)\n")
    fh.write("- all-signal monitoring (`monitor_signals`) where one-signal watchers are not equivalent\n\n")
    fh.write("## Portable files\n\n")
    if not portable:
        fh.write("- none\n")
    else:
        for row in sorted(portable, key=lambda item: (-item["test_count"], item["path"]))[:20]:
            tags = ",".join(row["tags"])
            fh.write(f"- `{row['path']}` - {row['test_count']} tests - axes `{tags}`\n")
    fh.write("\n## Portable with caveats\n\n")
    if not portable_with_caveats:
        fh.write("- none\n")
    else:
        for row in sorted(portable_with_caveats, key=lambda item: (-item["score"], item["path"]))[:20]:
            tags = ",".join(row["tags"])
            feats = ", ".join(row["features"][:4]) if row["features"] else "none"
            fh.write(f"- `{row['path']}` - {row['test_count']} tests - axes `{tags}` - caveats: {feats}\n")
    fh.write("\n## Exclude first\n\n")
    if not excluded:
        fh.write("- none\n")
    else:
        for row in sorted(excluded, key=lambda item: (-item["score"], item["path"]))[:20]:
            tags = ",".join(row["tags"])
            feats = ", ".join(row["features"][:5]) if row["features"] else "none"
            fh.write(f"- `{row['path']}` - {row['test_count']} tests - axes `{tags}` - blockers: {feats}\n")
    fh.write("\n## Recommended 20-30 test sample\n\n")
    if not selected:
        fh.write("- none\n")
    else:
        fh.write("| Suite | Tests | Cumulative | Status | Axes | Why |\n")
        fh.write("| --- | --- | --- | --- | --- | --- |\n")
        running = 0
        for row in selected:
            running += row["test_count"]
            why = ", ".join(row["features"][:4]) if row["features"] else "basic suite coverage"
            axes = ",".join(row["tags"])
            fh.write(f"| `{row['path']}` | {row['test_count']} | {running} | {row['status']} | `{axes}` | {why} |\n")
        fh.write(f"\nSelected sample size: `{running}` tests across `{len(selected)}` suites.\n")
PY
}

stage_suite_copies() {
    validate_paths
    "${PYTHON[@]}" - "$TEST_DIR" "$STAGED_TEST_DIR" "$RESULTS_DIR/gdunit4_suites.txt" "$HARNESS_PATH" "$RESULTS_DIR/gdtest_suite_targets.txt" <<'PY'
import pathlib
import re
import shutil
import sys

test_dir = pathlib.Path(sys.argv[1]).resolve()
staged_dir = pathlib.Path(sys.argv[2]).resolve()
suites_file = pathlib.Path(sys.argv[3])
harness_path = sys.argv[4].replace("\\", "/")
targets_file = pathlib.Path(sys.argv[5])
harness_dir = pathlib.PurePosixPath(harness_path).parent.as_posix()
compat_extends = f'extends "res://{harness_dir}/gdunit4_compat.gd"' if harness_dir != "." else 'extends "res://gdunit4_compat.gd"'

if staged_dir.exists():
    shutil.rmtree(staged_dir)
shutil.copytree(test_dir, staged_dir)

targets = []
for suite_line in suites_file.read_text(encoding="utf-8").splitlines():
    if not suite_line.strip():
        continue
    original = pathlib.Path(suite_line.strip()).resolve()
    rel = original.relative_to(test_dir)
    staged = staged_dir / rel
    text = staged.read_text(encoding="utf-8", errors="replace")
    updated, count = re.subn(r"(?m)^(\s*)extends\s+GdUnitTestSuite\b.*$", r"\1" + compat_extends, text, count=1)
    if count == 0:
        raise SystemExit(f"failed to rewrite extends line in {staged}")
    updated = re.sub(r"(?m)^\s*class_name\s+[A-Za-z0-9_]+\s*$\n?", "", updated, count=1)
    staged.write_text(updated, encoding="utf-8", newline="\n")
    targets.append(staged.as_posix())

targets_file.write_text("\n".join(targets) + ("\n" if targets else ""), encoding="utf-8", newline="\n")
PY
}

copy_baseline_xml() {
    local src
    src="$("${PYTHON[@]}" - "$BASELINE_REPORT_DIR" <<'PY'
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
if not root.exists():
    print("")
    raise SystemExit(0)
candidates = sorted(root.rglob("*.xml"), key=lambda path: (path.stat().st_mtime, path.as_posix()))
print(candidates[-1] if candidates else "")
PY
)"
    if [[ -n "$src" && -f "$src" ]]; then
        cp "$src" "$RESULTS_DIR/baseline_gdunit4_junit.xml"
        echo "ok"
    else
        printf '%s\n' '<testsuites/>' > "$RESULTS_DIR/baseline_gdunit4_junit.xml"
        echo "missing"
    fi
}

xml_status() {
    local target="$1"
    "${PYTHON[@]}" - "$target" <<'PY'
import sys
import xml.etree.ElementTree as ET

path = sys.argv[1]
try:
    ET.parse(path)
except FileNotFoundError:
    print("missing")
except ET.ParseError:
    print("invalid")
else:
    print("ok")
PY
}

write_readme() {
    local setup_ms="$1"
    local baseline_ms="$2"
    local gdtest_ms="$3"
    local baseline_exit="$4"
    local gdtest_exit="$5"
    local baseline_xml_state="$6"
    local gdtest_xml_state="$7"
    local ratio="$8"
    local suites
    suites="$(wc -l < "$RESULTS_DIR/gdunit4_suites.txt" | tr -d ' ')"
    cat > "$RESULTS_DIR/README.md" <<EOF
# gdUnit4 evaluation results

- Target project: \`$GAME_DIR\`
- Source test dir: \`$TEST_DIR\`
- Staged gdtest dir: \`$STAGED_TEST_DIR\`
- Harness dest: \`$HARNESS_DEST\`
- Harness path: \`$HARNESS_PATH\`
- gdtest jobs: \`$JOBS\`
- gdtest isolation: \`$ISOLATION\`
- Discovered suites: \`$suites\`

## Timings

- gdtest setup + staging: \`$setup_ms ms\`
- gdUnit4 baseline: \`$baseline_ms ms\`
- gdtest run: \`$gdtest_ms ms\`
- gdUnit4 / gdtest ratio: \`$ratio\`

## Exit codes

- gdUnit4 baseline: \`$baseline_exit\`
- gdtest JUnit run: \`$gdtest_exit\`

## XML/JUnit status

- baseline gdUnit4 JUnit: \`$baseline_xml_state\`
- gdtest JUnit: \`$gdtest_xml_state\`

## Files generated

- \`gdunit4_suites.txt\`
- \`assertion_stats.txt\`
- \`gdunit4_run.log\`
- \`baseline_gdunit4_junit.xml\`
- \`gdunit4_time_ms.txt\`
- \`gdtest_setup_timing.txt\`
- \`gdtest_junit.xml\`
- \`gdtest_time_ms.txt\`
- \`summary.env\`
- \`gdunit4_porting_candidates.md\`
- \`README.md\`
EOF
}

echo "== gdUnit4 discovery =="
write_kv "GAME_DIR" "$GAME_DIR"
write_kv "TEST_DIR" "$TEST_DIR"
write_kv "HARNESS_DEST" "$HARNESS_DEST"
write_kv "HARNESS_PATH" "$HARNESS_PATH"
write_kv "RESULTS_DIR" "$RESULTS_DIR"
write_kv "GDTEST_BIN" "$GDTEST_BIN"
write_kv "GODOT_EXE" "$GODOT_EXE"
discover_suites
build_inventory

TEST_DIR_RES="$(abs_to_res "$TEST_DIR")"
STAGED_TEST_DIR_RES="$(abs_to_res "$STAGED_TEST_DIR")"
write_kv "TEST_DIR_RES" "$TEST_DIR_RES"
write_kv "STAGED_TEST_DIR" "$STAGED_TEST_DIR"
write_kv "STAGED_TEST_DIR_RES" "$STAGED_TEST_DIR_RES"

echo "== Warm-up import =="
"$GODOT_EXE" --path "$GAME_DIR" --headless --import > "$RESULTS_DIR/import.txt" 2>&1 || true

echo "== gdUnit4 baseline =="
rm -rf "$BASELINE_REPORT_DIR"
mkdir -p "$BASELINE_REPORT_DIR"
BASELINE_START=$(now_ms)
BASELINE_EXIT=0
"$GODOT_EXE" --path "$GAME_DIR" --headless \
    -s res://addons/gdUnit4/bin/GdUnitCmdTool.gd \
    -a "$TEST_DIR_RES" \
    -rd "$BASELINE_REPORT_DIR" \
    --ignoreHeadlessMode \
    -c > "$RESULTS_DIR/gdunit4_run.log" 2>&1 || BASELINE_EXIT=$?
BASELINE_END=$(now_ms)
BASELINE_MS=$((BASELINE_END - BASELINE_START))
printf '%s\n' "$BASELINE_MS" > "$RESULTS_DIR/gdunit4_time_ms.txt"
write_kv "GDUNIT4_MS" "$BASELINE_MS"
write_kv "GDUNIT4_EXIT" "$BASELINE_EXIT"
BASELINE_XML_COPY="$(copy_baseline_xml)"
BASELINE_XML_STATE="$(xml_status "$RESULTS_DIR/baseline_gdunit4_junit.xml")"
write_kv "BASELINE_JUNIT_COPY" "$BASELINE_XML_COPY"
write_kv "BASELINE_XML_STATUS" "$BASELINE_XML_STATE"

echo "== gdtest setup =="
SETUP_START=$(now_ms)
copy_harness
stage_suite_copies
mapfile -t GDTEST_TARGETS < "$RESULTS_DIR/gdtest_suite_targets.txt"
write_kv "GDTEST_TARGET_COUNT" "${#GDTEST_TARGETS[@]}"
LIST_EXIT=0
"$GDTEST_BIN" "${GDTEST_TARGETS[@]}" \
    --godot-exe "$GODOT_EXE" \
    --project-path "$GAME_DIR" \
    --harness "$HARNESS_PATH" \
    --list > "$RESULTS_DIR/gdtest_setup_timing.txt" 2>&1 || LIST_EXIT=$?
SETUP_END=$(now_ms)
SETUP_MS=$((SETUP_END - SETUP_START))
{
    printf 'setup_ms=%s\n' "$SETUP_MS"
    printf 'gdtest_list_exit=%s\n' "$LIST_EXIT"
} >> "$RESULTS_DIR/gdtest_setup_timing.txt"
write_kv "SETUP_MS" "$SETUP_MS"
write_kv "GDTEST_LIST_EXIT" "$LIST_EXIT"

echo "== gdtest run + JUnit =="
GDTEST_START=$(now_ms)
GDTEST_EXIT=0
"$GDTEST_BIN" "${GDTEST_TARGETS[@]}" \
    --godot-exe "$GODOT_EXE" \
    --project-path "$GAME_DIR" \
    --harness "$HARNESS_PATH" \
    --reporter junit \
    --isolation "$ISOLATION" \
    -j "$JOBS" > "$RESULTS_DIR/gdtest_junit.xml" 2> "$RESULTS_DIR/gdtest_stderr.txt" || GDTEST_EXIT=$?
GDTEST_END=$(now_ms)
GDTEST_MS=$((GDTEST_END - GDTEST_START))
printf '%s\n' "$GDTEST_MS" > "$RESULTS_DIR/gdtest_time_ms.txt"
write_kv "GDTEST_MS" "$GDTEST_MS"
write_kv "GDTEST_EXIT" "$GDTEST_EXIT"
write_kv "JOBS" "$JOBS"
write_kv "ISOLATION" "$ISOLATION"

GDTEST_XML_STATE="$(xml_status "$RESULTS_DIR/gdtest_junit.xml")"
write_kv "GDTEST_XML_STATUS" "$GDTEST_XML_STATE"

RATIO="$(safe_ratio "$BASELINE_MS" "$GDTEST_MS")"
write_kv "GDUNIT4_OVER_GDTEST" "$RATIO"

write_readme \
    "$SETUP_MS" \
    "$BASELINE_MS" \
    "$GDTEST_MS" \
    "$BASELINE_EXIT" \
    "$GDTEST_EXIT" \
    "$BASELINE_XML_STATE" \
    "$GDTEST_XML_STATE" \
    "$RATIO"

echo
echo "Results written to $RESULTS_DIR"
echo "Setup:   ${SETUP_MS} ms"
echo "gdUnit4: ${BASELINE_MS} ms"
echo "gdtest:  ${GDTEST_MS} ms"
echo "Ratio:   ${RATIO}"
echo "XML:     baseline=${BASELINE_XML_STATE} gdtest=${GDTEST_XML_STATE}"
