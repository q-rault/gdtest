# Benchmarks

`bench.sh` times gdtest, GUT, and gdUnit4 against the same test suite and prints a comparison table with gdtest as the reference point.

## Prerequisites

- `GODOT_EXE` set in `.env` at the repo root or in your environment
- `cargo build --release` done (or `just build`)
- `GODOT_PROJECT_PATH` pointing to a Godot project with GUT and gdUnit4 installed
- Optional: `GDTEST_BIN` if you want to benchmark a binary outside `target/release/gdtest`

## Usage

```sh
# Full suite, all tools, 3 runs (default)
benchmarks/bench.sh --tool all --category all

# Quick smoke run (1 run)
BENCH_RUNS=1 benchmarks/bench.sh --tool all

# gdtest vs GUT only, pure-logic category
benchmarks/bench.sh --tool gdtest,gut --category pure-logic

# Parallelism sweep across j=1, 4, 8
benchmarks/bench.sh --j-sweep

# Benchmark your own project from a manifest file
GODOT_PROJECT_PATH=/path/to/game MANIFEST=/path/to/bench.manifest \
  benchmarks/bench.sh --tool all --manifest /path/to/bench.manifest
```

Or use the Justfile recipes:

```sh
# Internal suite (reads GODOT_PROJECT_PATH from .env)
just bench

# Your game's suite
GODOT_PROJECT_PATH=~/my-game MANIFEST=~/my-game/bench.manifest just bench-project
```

## Flags

| Flag | Default | Effect |
| --- | --- | --- |
| `--tool gdtest\|gut\|gdunit4\|all` | `gdtest,gut` | Which runners to include |
| `--category pure-logic\|scene\|physics\|isolation\|all` | `all` | Test subset |
| `--runs N` | `3` (or `$BENCH_RUNS`) | Timing samples per cell |
| `-j N` | `4` | Worker count for gdtest |
| `--j-sweep` | off | Run at j=1, 4, 8 |
| `--manifest FILE` | none | One `path/to/test.gd [file\|test]` line per file |
| `--no-warmup` | off | Skip the initial `godot --import` pass |

`BENCH_RUNS` sets the default run count. This is useful in CI: `BENCH_RUNS=1 just bench`.

## Output

```text
category      tool        tests     j    median      min      max    vs gdtest
-----------------------------------------------------------------------------
all           gdtest         61     4   13.014s  12.992s  13.143s          ref
all           gut            58    --   27.792s  27.733s  27.794s        2.14x
all           gdunit4        61    --   24.411s  24.408s  24.458s        1.88x
all           gut-rt         61    --   13.025s  12.991s  13.236s        1.00x
all           gdunit4-rt     61    --   13.018s  12.999s  13.128s        1.00x

SUMMARY: gdtest vs gut - gdtest: 61 tests / gut: 58 tests - counts differ, comparison excluded
SUMMARY: gdtest vs gdunit4 - both ran 61 tests - gdtest is 1.88x faster (13.0s vs 24.4s)
```

`vs gdtest` is `tool_time / gdtest_time`. Values above `1.00x` mean gdtest is faster.

The `SUMMARY:` lines are grep-friendly for CI log parsing.

## Choosing tests

Pass `--manifest /path/to/bench.manifest` to benchmark your own project's tests. The `--category` presets in `bench.sh` name files from a specific test suite and only work on a project that has them.

## Saved results

JSON runs land in `benchmarks/results/` (`.gitignore`'d; only `.gitkeep` is tracked).

## Transpiler

`transpile.py` converts test files between GdTest, GUT, and gdUnit4.

```sh
# Single file
just transpile --target gut     test/cases/test_player.gd > out.gd   # GdTest -> GUT
just transpile --source gut     test_player_gut.gd > out.gd          # GUT -> GdTest
just transpile --source gdunit4 test_player_gdunit4.gd > out.gd      # gdUnit4 -> GdTest

# Directory - rewrites files in place (use git to revert)
just transpile --source gut     test/cases/
just transpile --source gdunit4 --check test/cases/   # dry-run, exit 1 if diff

# Directory - write converted files to an output directory
just transpile --target gut --output-dir /tmp/gut-tests test/cases/
```

The `--check` flag prints a unified diff and exits `1` if any file would change, without writing anything.

## Evaluation kit

Use the templates in `benchmarks/templates/` when evaluating a real project for migration fit:

- `game-scorecard.md`: 7-axis scorecard with scoring rubric and verdict bands
- `metric-collection-checklist.md`: commands and evidence to collect per axis
- `api-mapping-template.md`: per-project source API to gdtest mapping table
- `gut-migration-notes.md`: per-file migration notes for representative GUT ports
- `gdunit4-migration-notes.md`: per-file migration notes for representative gdUnit4 ports

Recommended flow:

1. Run the checklist and keep the raw outputs.
2. Fill the API mapping table while porting representative tests.
3. Score the project only after you have evidence for all 7 axes.

## GUT project evaluation

For a GUT-based game, use `benchmarks/eval_gut_game.sh` to generate the raw artifacts for axes A, B, E, and F in one pass.

```sh
export GAME_DIR=/path/to/game
export GODOT_EXE=/path/to/godot
export GDTEST_BIN=/path/to/gdtest

benchmarks/eval_gut_game.sh
```

Outputs land in `benchmarks/results/<game>-gut-eval/` and include:

- `gdtest_list.txt` for setup validation
- `gut_output.txt` and `gdtest_junit.xml` for runtime and CI evidence
- `gut_assertions.csv` for the assertion/helper inventory
- `gut_porting_candidates.md` for choosing representative files to port
- `summary.env` and `README.md` with timings and exit codes

Use `benchmarks/templates/gut-migration-notes.md` while porting the chosen files so the scorecard can point at concrete rewrite examples.

## gdUnit4 project evaluation

For a gdUnit4-based game, use `benchmarks/eval_gdunit4_game.sh` to generate the raw artifacts for suite discovery, API inventory, baseline runtime, gdtest setup, and gdtest JUnit output in one pass.

The harness is generic. Actual runs depend on those repositories being cloned locally as Godot projects with their gdUnit4 suites available on disk.

```sh
export GAME_DIR=/path/to/game
export GODOT_EXE=/path/to/godot
export GDTEST_BIN=/path/to/gdtest

benchmarks/eval_gdunit4_game.sh
```

Outputs land in `benchmarks/results/<game>-gdunit4-eval/` and include:

- `gdunit4_suites.txt` for suite discovery
- `assertion_stats.txt` for the API/helper/blocker inventory
- `gdunit4_run.log` and `baseline_gdunit4_junit.xml` for source-runner evidence
- `gdtest_setup_timing.txt`, `gdtest_junit.xml`, and `gdtest_time_ms.txt` for gdtest evidence
- `gdunit4_porting_candidates.md` for portable vs excluded files and the 20-30 test sample
- `summary.env` and `README.md` with timings, exit codes, and the gdUnit4/gdtest ratio

Use `benchmarks/templates/gdunit4-migration-notes.md` while porting the chosen files so the scorecard can point at concrete rewrite examples.
