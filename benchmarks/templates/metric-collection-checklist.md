# gdtest Evaluation - Metric Collection Checklist

Use this checklist before filling the scorecard.

## Common setup

```sh
export GAME_DIR=/path/to/game
export TEST_DIR="$GAME_DIR/test"
export GDTEST_BIN=/path/to/gdtest
export GODOT_EXE=/path/to/godot
```

- Confirm the project opens headless with `godot --headless --import`.
- Record the source framework (`GUT` or `gdUnit4`) and the test directory.
- Keep raw outputs in a scratch directory so every score links back to evidence.

## Axis A - Setup friction

Goal: measure install time, manual steps, and blockers.

```sh
time (
  mkdir -p "$GAME_DIR/addons/gdtest" &&
  cp addons/gdtest/gd_test.gd addons/gdtest/gdt.gd addons/gdtest/gdtest_worker.gd "$GAME_DIR/addons/gdtest/" &&
  "$GDTEST_BIN" "$TEST_DIR" --project-path "$GAME_DIR" --list
)
```

Record:
- Start/end time
- Manual edits required outside copying the harness
- Import/plugin errors, if any

## Axis B - API coverage

Goal: compare assertions/patterns used by the source suite against gdtest coverage.

```sh
rg -o "(assert|expect)_[A-Za-z0-9_]+" "$TEST_DIR" -g "*.gd" \
  | sort | uniq -c | sort -rn > /tmp/assertions_used.txt

rg -o "func (assert|expect)_[A-Za-z0-9_]+" harness -g "*.gd" \
  | sed 's/^func //' | sort | uniq > /tmp/assertions_gdtest.txt

comm -23 <(awk '{print $2}' /tmp/assertions_used.txt | sort) /tmp/assertions_gdtest.txt \
  > /tmp/assertion_gaps.txt
```

Record:
- Covered assertions / total unique assertions used
- Gap list from `/tmp/assertion_gaps.txt`
- Whether each gap has a simple workaround

## Axis C - Migration effort

Goal: quantify how invasive the port is.

```sh
find "$TEST_DIR" -name "*.gd" -print0 | xargs -0 wc -l
git diff --stat -- "$TEST_DIR"
git diff --word-diff -- "$TEST_DIR"
```

Record:
- Total test LOC before migration
- Files touched
- Lines added/removed
- Repeated rewrite patterns (header swap, await helper swap, assertion rename)

## Axis D - Failure quality

Goal: inspect the usefulness of failing output.

```sh
"$GDTEST_BIN" "$TEST_DIR/test_sample.gd" --project-path "$GAME_DIR" 2>&1 \
  | tee /tmp/gdtest-failure.txt
```

Record:
- Whether the output includes file path
- Whether the output includes line number
- Whether the output includes expected/actual diff
- Whether the output is actionable without rerunning in verbose mode

## Axis E - Performance

Goal: compare wall-clock runtime against the original framework.

```sh
# Source runner baseline
time godot --headless --path "$GAME_DIR" -s addons/gut/gut_cmdln.gd -gdir="$TEST_DIR" -gexit

# gdtest baseline
time "$GDTEST_BIN" "$TEST_DIR" --project-path "$GAME_DIR" --reporter tap

# Optional full comparison harness
GODOT_PROJECT_PATH="$GAME_DIR" benchmarks/bench.sh --tool all --category all
```

Record:
- Median runtime per runner
- `-j` value used for gdtest
- Whether any category needed `--isolation=test`

## Axis F - CI readiness

Goal: confirm automation output is clean and machine-readable.

```sh
"$GDTEST_BIN" "$TEST_DIR" --project-path "$GAME_DIR" --reporter junit > /tmp/gdtest-results.xml
echo "exit=$?"
xmllint --noout /tmp/gdtest-results.xml
```

Record:
- Exit code on pass/fail
- Headless behavior in CI
- JUnit XML validity
- Any quoting/path issues in GitHub Actions or the target CI system

## Axis G - Advanced features

Goal: probe the project-specific features that usually break migrations.

```gdscript
extends GdTest

func test_mock_probe():
    var node := mock(Node)
    node.get_name()
    verify(node).get_name()

func test_signal_probe():
    var emitter := Node.new()
    emitter.add_user_signal("done")
    watch_signals(emitter)
    emitter.emit_signal("done")
    assert_signal_emitted(emitter, "done")

func test_async_probe():
    await wait_process_frame()
    assert_true(true)
```

Also test:
- Scene helpers via `SceneRunner` for input-driven tests
- Autoload/global-state cleanup requirements
- Snapshot or filesystem helpers if the suite relies on them

Record:
- Covered areas: mocking, signals, async, scenes
- Missing helpers
- Required workarounds
