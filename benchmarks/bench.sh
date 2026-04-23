#!/usr/bin/env bash
# benchmarks/bench.sh — timing runner.
# Writes one TSV line per result to stdout:
#   category  tool  tests  j  median_ms  min_ms  max_ms
# Pipe into benchmarks/report.py for the formatted table.
#
# Usage:
#   benchmarks/bench.sh [--category pure-logic|scene|physics|isolation|all]
#                       [--tool gdtest|gut|gdunit4|all]
#                       [-j JOBS] [--j-sweep] [--runs N] [--no-warmup]
#
# Called by `just bench`; env vars below are set there.
# Can also be run standalone if you export them manually.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GDTEST_ROOT="$(dirname "$SCRIPT_DIR")"

if [[ -f "$GDTEST_ROOT/.env" ]]; then source "$GDTEST_ROOT/.env"; fi

# GODOT_PROJECT_PATH — Godot project to benchmark (your game repo).
# BENCH_PROJECT_PATH is accepted as a legacy alias for the gdtest dev setup.
: "${GODOT_PROJECT_PATH:=${BENCH_PROJECT_PATH:-}}"
: "${GODOT_PROJECT_PATH:?set GODOT_PROJECT_PATH=/path/to/benchmark-project}"
: "${GDTEST_BIN:=$GDTEST_ROOT/target/release/gdtest}"
: "${GODOT_EXE:?GODOT_EXE must be set in .env or environment}"
: "${GDTEST_HARNESS:=addons/gdtest/gdtest_worker.gd}"
: "${RUNS:=3}"
: "${JOBS:=4}"
: "${PYTHON3:=python3}"
export PYTHONUTF8=1  # force UTF-8 stdout/stderr for all Python child processes

[[ -x "$GDTEST_BIN" ]] || { echo "missing $GDTEST_BIN — run: cargo build --release" >&2; exit 1; }

TRANSPILE="$SCRIPT_DIR/transpile.py"
BENCH_TMP="$GODOT_PROJECT_PATH/.gdtest_bench"
_BENCH_REL=".gdtest_bench"          # project-relative path used by GUT/gdUnit4 flags
GUT_BENCH_RES="res://$_BENCH_REL"

# ── Arg parsing ───────────────────────────────────────────────────────────────
CATEGORIES="all"
TOOLS="gdtest,gut"
J_SWEEP=false
WARMUP=true
MANIFEST=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --category)  CATEGORIES="$2"; shift 2 ;;
        --tool)      TOOLS="$2";      shift 2 ;;
        -j)          JOBS="$2";       shift 2 ;;
        --j-sweep)   J_SWEEP=true;    shift ;;
        --runs)      RUNS="$2";       shift 2 ;;
        --no-warmup) WARMUP=false;    shift ;;
        --manifest)  MANIFEST="$2";   shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

# ── Manifest support ──────────────────────────────────────────────────────────
# Probes gdtest --list to count tests in the given files.
_probe_test_count() {
    [[ $# -eq 0 ]] && echo 0 && return
    "$GDTEST_BIN" --godot-exe "$GODOT_EXE" \
        --project-path "$GODOT_PROJECT_PATH" \
        --harness "$GDTEST_HARNESS" \
        --list "$@" 2>/dev/null | grep -c ' :: ' || echo 0
}

# Reads a manifest file (one "path [isolation]" line per test file, isolation defaults to file)
# and populates the category arrays used by run_one().
load_manifest() {
    local mf="$1"
    local -a fa=() fb=()
    while read -r path iso _rest; do
        [[ -z "$path" || "$path" == \#* ]] && continue
        if [[ "${iso:-file}" == "test" ]]; then fb+=("$path"); else fa+=("$path"); fi
    done < "$mf"
    PL_A=("${fa[@]}"); PL_N=$(_probe_test_count "${fa[@]}")
    IS_B=("${fb[@]}"); IS_N=$(_probe_test_count "${fb[@]}")
    SC_A=(); SC_N=0; PH_A=(); PH_N=0
    ALL_N=$(( PL_N + IS_N ))
    CATEGORIES="all"
    echo "== Manifest loaded: ${#fa[@]} file-isolation files ($PL_N tests), ${#fb[@]} test-isolation files ($IS_N tests) ==" >&2
}

if [[ -n "$MANIFEST" ]]; then
    load_manifest "$MANIFEST"
fi

# ── Timing helpers (integer milliseconds — no Python needed) ──────────────────
now_ms()  { date +%s%3N; }
median()  { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
minval()  { printf '%s\n' "$@" | sort -n | head -1; }
maxval()  { printf '%s\n' "$@" | sort -n | tail -1; }
add_ms()  { echo $(( $1 + $2 )); }

# Cross-platform timeout: GNU coreutils, gtimeout (brew), or perl alarm.
safe_timeout() {
    local secs="$1"; shift
    if   command -v gtimeout >/dev/null 2>&1; then gtimeout "$secs" "$@"
    elif command -v timeout  >/dev/null 2>&1; then timeout  "$secs" "$@"
    else perl -e "alarm($secs); exec @ARGV" -- "$@"
    fi
}

# ── Category → file mapping ───────────────────────────────────────────────────
# Group A: --isolation=file   Group B: --isolation=test
PL_A=(test/cases/test_armure_mr.gd);                                          PL_N=16
SC_A=(test/cases/test_minions.gd test/cases/test_abilities_matrix.gd);        SC_N=6
PH_A=(test/cases/test_auto_attack.gd);                                         PH_N=5
IS_B=(test/cases/test_economie_or.gd test/cases/test_respawn_timer_de_mort.gd); IS_N=34
ALL_N=$(( PL_N + SC_N + PH_N + IS_N ))

# ── Tool runners — return "median_ms|min_ms|max_ms" ──────────────────────────

time_gdtest() {
    local isolation="$1" jobs="$2"; shift 2
    local -a cmd=("$GDTEST_BIN"
        --godot-exe "$GODOT_EXE"
        --project-path "$GODOT_PROJECT_PATH"
        --harness "$GDTEST_HARNESS"
        --no-auto-import
        -j "$jobs"
    )
    [[ "$isolation" == "test" ]] && cmd+=(--isolation test)
    cmd+=("$@")
    local -a times=()
    for (( run=1; run<=RUNS; run++ )); do
        local t0 t1; t0=$(now_ms)
        "${cmd[@]}" >/dev/null 2>&1 || true
        t1=$(now_ms); times+=("$(( t1 - t0 ))")
    done
    echo "$(median "${times[@]}")|$(minval "${times[@]}")|$(maxval "${times[@]}")"
}

time_gut_from_gdtest() {
    mkdir -p "$BENCH_TMP"
    for f in "$@"; do
        "$PYTHON3" "$TRANSPILE" --target gut "$GODOT_PROJECT_PATH/$f" > "$BENCH_TMP/$(basename "$f")"
    done
    local probe n_actual
    probe=$("$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless \
        -s res://addons/gut/gut_cmdln.gd -- -gdir="$GUT_BENCH_RES" -gexit \
        2>&1) || true
    # Captures passing tests. One test (cancel_windup) fails in GUT due to physics delta
    # variance in headless mode committing the windup early — acceptable best-effort gap.
    n_actual=$(echo "$probe" | grep -oP 'Passing Tests\s+\K\d+' || echo "0")
    local -a times=()
    for (( run=1; run<=RUNS; run++ )); do
        local t0 t1; t0=$(now_ms)
        "$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless \
            -s res://addons/gut/gut_cmdln.gd -- -gdir="$GUT_BENCH_RES" -gexit \
            >/dev/null 2>&1 || true
        t1=$(now_ms); times+=("$(( t1 - t0 ))")
    done
    rm -rf "$BENCH_TMP"
    echo "$(median "${times[@]}")|$(minval "${times[@]}")|$(maxval "${times[@]}")|$n_actual"
}

time_gdunit4_from_gdtest() {
    [[ -d "$GODOT_PROJECT_PATH/addons/gdUnit4" ]] || { echo "skip"; return; }
    mkdir -p "$BENCH_TMP"
    for f in "$@"; do
        "$PYTHON3" "$TRANSPILE" --target gdunit4 "$GODOT_PROJECT_PATH/$f" > "$BENCH_TMP/$(basename "$f")"
    done
    local probe n_actual
    probe=$(safe_timeout 120 "$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless \
        -s res://addons/gdUnit4/bin/GdUnitCmdTool.gd \
        -a "$_BENCH_REL" --ignoreHeadlessMode -c \
        2>&1) || true
    # gdunit4 prints "(6/6)" for test suites then "(61/61)" for test cases on separate lines;
    # filter to the "test cases" line so we capture case count, not suite count.
    n_actual=$(echo "$probe" | grep -i 'test cases' | grep -oP '\d+(?=/\d+\))' | head -1 || echo "0")
    local -a times=()
    for (( run=1; run<=RUNS; run++ )); do
        local t0 t1; t0=$(now_ms)
        safe_timeout 120 "$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless \
            -s res://addons/gdUnit4/bin/GdUnitCmdTool.gd \
            -a "$_BENCH_REL" --ignoreHeadlessMode -c \
            >/dev/null 2>&1 || true
        t1=$(now_ms); times+=("$(( t1 - t0 ))")
    done
    rm -rf "$BENCH_TMP"
    echo "$(median "${times[@]}")|$(minval "${times[@]}")|$(maxval "${times[@]}")|$n_actual"
}

merge_ms() {
    local m1 n1 x1 na1 m2 n2 x2 na2
    IFS='|' read -r m1 n1 x1 na1 <<< "$1"
    IFS='|' read -r m2 n2 x2 na2 <<< "$2"
    local merged; merged="$(add_ms "$m1" "$m2")|$(add_ms "$n1" "$n2")|$(add_ms "$x1" "$x2")"
    [[ -n "${na1}${na2}" ]] && merged+="|$(( ${na1:-0} + ${na2:-0} ))"
    echo "$merged"
}

# Round-trip: GdTest → via (gut|gdunit4) → GdTest → run with gdtest.
# Verifies that the --source direction produces equivalent runnable output.
time_gdtest_roundtrip() {
    local via="$1" isolation="$2" jobs="$3"; shift 3
    mkdir -p "$BENCH_TMP"
    local -a rt_files=()
    for f in "$@"; do
        local base; base=$(basename "$f")
        local via_f="$BENCH_TMP/_via_${via}_${base}"
        local rt_f="$BENCH_TMP/_rt_${via}_${base}"
        "$PYTHON3" "$TRANSPILE" --target "$via" "$GODOT_PROJECT_PATH/$f" > "$via_f"
        "$PYTHON3" "$TRANSPILE" --source "$via" "$via_f" > "$rt_f"
        rt_files+=("$_BENCH_REL/_rt_${via}_${base}")
    done
    # One-shot import so timed runs can use --no-auto-import (same pattern as top-level warmup).
    "$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless --import >/dev/null 2>&1 || true
    local -a cmd=("$GDTEST_BIN"
        --godot-exe "$GODOT_EXE"
        --project-path "$GODOT_PROJECT_PATH"
        --harness "$GDTEST_HARNESS"
        --no-auto-import
        -j "$jobs"
    )
    [[ "$isolation" == "test" ]] && cmd+=(--isolation test)
    cmd+=("${rt_files[@]}")
    # Probe run: captures actual test count (files already imported above).
    local probe; probe=$("${cmd[@]}" 2>&1) || true
    local n_actual
    n_actual=$(echo "$probe" \
        | sed 's/\x1b\[[0-9;]*[mK]//g' \
        | awk '/^[[:space:]]*[0-9]+ tests?[[:space:]]/{print $1+0; exit}')
    n_actual="${n_actual:-0}"
    local -a times=()
    for (( run=1; run<=RUNS; run++ )); do
        local t0 t1; t0=$(now_ms)
        "${cmd[@]}" >/dev/null 2>&1 || true
        t1=$(now_ms); times+=("$(( t1 - t0 ))")
    done
    rm -rf "$BENCH_TMP"
    echo "$(median "${times[@]}")|$(minval "${times[@]}")|$(maxval "${times[@]}")|$n_actual"
}

# ── Per-category runner ───────────────────────────────────────────────────────

run_one() {
    local cat="$1" tool="$2" jobs="$3"
    local -a gdt_a=() gdt_b=()
    local n_tests

    case "$cat" in
        pure-logic) gdt_a=("${PL_A[@]}");                              n_tests=$PL_N ;;
        scene)      gdt_a=("${SC_A[@]}");                              n_tests=$SC_N ;;
        physics)    gdt_a=("${PH_A[@]}");                              n_tests=$PH_N ;;
        isolation)  gdt_b=("${IS_B[@]}");                              n_tests=$IS_N ;;
        all)        gdt_a=("${PL_A[@]}" "${SC_A[@]}" "${PH_A[@]}")
                    gdt_b=("${IS_B[@]}");                              n_tests=$ALL_N ;;
    esac

    local -a all_gdt=("${gdt_a[@]}" "${gdt_b[@]}")
    printf '  [%-10s] %-8s ... ' "$cat" "$tool" >&2

    local result
    case "$tool" in
        gut)
            result=$(time_gut_from_gdtest "${all_gdt[@]}")
            ;;
        gdtest)
            local has_a=${#gdt_a[@]} has_b=${#gdt_b[@]}
            if   (( has_a > 0 && has_b > 0 )); then
                result=$(merge_ms \
                    "$(time_gdtest "file" "$jobs" "${gdt_a[@]}")" \
                    "$(time_gdtest "test" "$jobs" "${gdt_b[@]}")")
            elif (( has_a > 0 )); then result=$(time_gdtest "file" "$jobs" "${gdt_a[@]}")
            else                       result=$(time_gdtest "test" "$jobs" "${gdt_b[@]}")
            fi
            ;;
        gdunit4)
            result=$(time_gdunit4_from_gdtest "${all_gdt[@]}")
            if [[ "$result" == "skip" ]]; then
                echo "skipped (addons/gdUnit4 not installed)" >&2; return
            fi
            ;;
        gut-rt|gdunit4-rt)
            local via="${tool%-rt}"
            local has_a=${#gdt_a[@]} has_b=${#gdt_b[@]}
            if   (( has_a > 0 && has_b > 0 )); then
                result=$(merge_ms \
                    "$(time_gdtest_roundtrip "$via" "file" "$jobs" "${gdt_a[@]}")" \
                    "$(time_gdtest_roundtrip "$via" "test" "$jobs" "${gdt_b[@]}")")
            elif (( has_a > 0 )); then result=$(time_gdtest_roundtrip "$via" "file" "$jobs" "${gdt_a[@]}")
            else                       result=$(time_gdtest_roundtrip "$via" "test" "$jobs" "${gdt_b[@]}")
            fi
            ;;
    esac

    local med min max n_actual
    IFS='|' read -r med min max n_actual <<< "$result"
    printf '%dms  (min %dms  max %dms)\n' "$med" "$min" "$max" >&2
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$cat" "$tool" "${n_actual:-$n_tests}" "$jobs" "$med" "$min" "$max"
}

# ── Entry point ───────────────────────────────────────────────────────────────

if $WARMUP; then
    echo "== Warm-up: Godot import cache ==" >&2
    "$GODOT_EXE" --path "$GODOT_PROJECT_PATH" --headless --import >/dev/null 2>&1 || true
fi

declare -a active_cats active_tools
[[ "$CATEGORIES" == "all" ]] && active_cats=(all)   || IFS=',' read -ra active_cats <<< "$CATEGORIES"
[[ "$TOOLS"      == "all" ]] && active_tools=(gdtest gut gdunit4 gut-rt gdunit4-rt) || IFS=',' read -ra active_tools <<< "$TOOLS"

run_sweep() {
    local j="$1"
    for cat in "${active_cats[@]}"; do
        for tool in "${active_tools[@]}"; do run_one "$cat" "$tool" "$j"; done
    done
}

echo "" >&2
if $J_SWEEP; then
    for j in 1 4 8; do
        echo "══ j=$j ════════════════════════════════════════════" >&2
        run_sweep "$j"
        echo "" >&2
    done
else
    run_sweep "$JOBS"
fi
