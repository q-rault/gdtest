#!/usr/bin/env python3
"""benchmarks/report.py — format bench.sh TSV output as table + JSON.

Reads TSV from stdin:  category  tool  tests  j  median_ms  min_ms  max_ms

Usage (called by `just bench`):
    benchmarks/bench.sh ... | python3 benchmarks/report.py [--save]
                                          [--gdtest-bin PATH] [--godot-exe PATH]
                                          [--runs N]
"""
import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path


def ms_to_s(ms: str) -> float:
    return round(int(ms) / 1000, 3)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", action="store_true")
    ap.add_argument("--gdtest-bin", default="./target/release/gdtest")
    ap.add_argument("--godot-exe",  default="")
    ap.add_argument("--runs", type=int, default=3)
    args = ap.parse_args()

    rows = [ln.rstrip("\n").split("\t") for ln in sys.stdin if ln.strip()]
    if not rows:
        print("  (no results)")
        return

    display = [(r[0], r[1], r[2], r[3], ms_to_s(r[4]), ms_to_s(r[5]), ms_to_s(r[6]))
               for r in rows]
    gdtest_med = {r[0]: ms_to_s(r[4]) for r in rows if r[1] == "gdtest"}

    hdr = (f"{'category':<12}  {'tool':<10}  {'tests':>5}  {'j':>4}"
           f"  {'median':>8}  {'min':>7}  {'max':>7}  {'vs gdtest':>10}")
    print(hdr)
    print("-" * len(hdr))
    for cat, tool, tests, j, med, mn, mx in display:
        j_disp   = j if tool == "gdtest" else "--"
        speedup  = "ref" if tool == "gdtest" else (
            f"{med / gdtest_med[cat]:.2f}×" if cat in gdtest_med else "—")
        print(f"{cat:<12}  {tool:<10}  {tests:>5}  {j_disp:>4}"
              f"  {med:>7}s  {mn:>6}s  {mx:>6}s  {speedup:>10}")

    gdt_meds      = [ms_to_s(r[4]) for r in rows if r[1] == "gdtest"]
    gdt_tests     = [int(r[2])     for r in rows if r[1] == "gdtest"]
    gut_meds      = [ms_to_s(r[4]) for r in rows if r[1] == "gut"]
    gut_tests     = [int(r[2])     for r in rows if r[1] == "gut"]
    gdunit4_meds  = [ms_to_s(r[4]) for r in rows if r[1] == "gdunit4"]
    gdunit4_tests = [int(r[2])     for r in rows if r[1] == "gdunit4"]
    if gdt_meds:
        avg = sum(gdt_meds) / len(gdt_meds)
        g_tests_avg = sum(gdt_tests) / len(gdt_tests)
        print()

        def _summary(tool: str, tool_meds: list[float], tool_tests: list[int]) -> None:
            t_avg = sum(tool_meds) / len(tool_meds)
            t_tests_avg = sum(tool_tests) / len(tool_tests)
            if g_tests_avg != t_tests_avg:
                print(f"SUMMARY: gdtest vs {tool} — gdtest: {int(g_tests_avg)} tests / "
                      f"{tool}: {int(t_tests_avg)} tests — counts differ, comparison excluded")
                return
            n = int(g_tests_avg)
            ratio = t_avg / avg
            if ratio >= 1:
                winner = f"gdtest is {ratio:.2f}x faster ({avg:.1f}s vs {t_avg:.1f}s)"
            else:
                winner = f"{tool} is {1/ratio:.2f}x faster ({t_avg:.1f}s vs {avg:.1f}s)"
            print(f"SUMMARY: gdtest vs {tool} — both ran {n} tests — {winner}")

        if gut_meds:
            _summary("gut", gut_meds, gut_tests)
        if gdunit4_meds:
            _summary("gdunit4", gdunit4_meds, gdunit4_tests)

    if not args.save:
        return

    def run(cmd: list[str]) -> str:
        try:
            return subprocess.check_output(cmd, stderr=subprocess.DEVNULL, text=True).strip()
        except Exception:
            return "unknown"

    ver  = run([args.gdtest_bin, "--version"]).split()[-1] if args.gdtest_bin else "unknown"
    gver = run([args.godot_exe, "--version"]) if args.godot_exe else "unknown"

    results = []
    for r in rows:
        cat, tool, tests, j, med_ms, mn_ms, mx_ms = r
        entry: dict = {
            "tool": tool, "category": cat, "tests": int(tests),
            "jobs": int(j) if tool == "gdtest" else None,
            "median_s": ms_to_s(med_ms), "min_s": ms_to_s(mn_ms), "max_s": ms_to_s(mx_ms),
        }
        if tool != "gdtest" and cat in gdtest_med:
            entry["gdtest_speedup"] = round(ms_to_s(med_ms) / gdtest_med[cat], 2)
        results.append(entry)

    payload = {
        "timestamp":      datetime.datetime.now(datetime.UTC).isoformat() + "Z",
        "gdtest_version": ver,
        "godot_version":  gver,
        "runs":           args.runs,
        "results":        results,
    }

    results_dir = Path(__file__).parent / "results"
    results_dir.mkdir(exist_ok=True)
    ts    = datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%d-%H%M%S")
    saved = results_dir / f"{ts}.json"
    saved.write_text(json.dumps(payload, indent=2))
    print(f"\nSaved: {saved}")


if __name__ == "__main__":
    main()
