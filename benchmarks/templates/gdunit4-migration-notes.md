# [Game Name] - gdUnit4 Migration Notes

- Repository: `https://github.com/...`
- Evaluation date: `YYYY-MM-DD`
- Evaluator: `name`
- Source test directory: `res://test`

Use one section per migrated source file. Keep the notes concrete and tie them
back to the raw evidence produced by `benchmarks/eval_gdunit4_game.sh`.

## File: `res://path/to/source_test.gd`

| Test | gdUnit4 call or pattern | gdtest equivalent | Effort (min) | Friction | Notes |
| --- | --- | --- | --- | --- | --- |
| `test_basic` | `extends GdUnitTestSuite` | `extends GdTestCompatGdUnit` | `1` | None | Base class swap only |
| `test_signal` | `monitor_signals(node)` | `watch_signal(node, "done")` | `10` | Medium | Rewrite per watched signal |
| `test_verify` | `verify(mock, 2).fire()` | `assert_call_count(mock, "fire", 2)` | `5` | Low | Same intent, different call shape |
| `test_scene` | `scene_runner("res://ui/panel.tscn")` | `scene_runner(load("res://ui/panel.tscn"))` | `5` | Low | Path string becomes `PackedScene` |
| `test_params` | `@GdUnitTestParameters([...])` | none | `30` | Blocking | Split into manual cases or helper loop |

Friction scale:

- `None`: less than 1 minute, pure mechanical rename
- `Low`: 1 to 5 minutes, simple rewrite
- `Medium`: 5 to 15 minutes, changed calling pattern or setup
- `High`: 15 to 30 minutes, notable rewrite or workaround
- `Blocking`: cannot migrate cleanly with the current harness

## Repeated rewrite patterns

- `extends GdUnitTestSuite` -> `extends GdTestCompatGdUnit`
- `verify(mock, n).method()` -> `assert_call_count(mock, "method", n)`
- `scene_runner("res://foo.tscn")` -> `scene_runner(load("res://foo.tscn"))`
- `simulate_*_release` -> gdtest `*_released` helpers
- `monitor_signals(obj)` -> one `watch_signal(obj, sig)` per signal under test

## Blocking gaps

Record only blockers confirmed by the real suite:

- `@GdUnitTestParameters`
- `GdUnitTuple` / `tuple(...)`
- `InputFactory.*`
- deep stubs (`RETURN_DEEP_STUB`)
- helpers de fichiers/tempdir non portes proprement
