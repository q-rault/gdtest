# [Game Name] - GUT Migration Notes

- Repository: `https://github.com/...`
- Evaluation date: `YYYY-MM-DD`
- Evaluator: `name`
- Source test directory: `res://test`

Use one section per migrated source file. Keep the notes concrete and tie them
back to the raw evidence produced by `benchmarks/eval_gut_game.sh`.

## File: `res://path/to/source_test.gd`

| Test | GUT call or pattern | gdtest equivalent | Effort (min) | Friction | Notes |
| --- | --- | --- | --- | --- | --- |
| `test_foo` | `assert_eq(hp, 100)` | `assert_eq(hp, 100)` | `1` | None | Direct port |
| `test_bar` | `watch_signals(node)` + `assert_signal_emitted(node, "died")` | `watch_signal(node, "died")` + `await assert_signal(...)` | `15` | High | Whole-object watcher becomes per-signal setup |
| `test_async` | `yield_to(loader, "loaded", 2.0)` | `await assert_signal(loader, "loaded", 2000)` | `10` | Medium | Wait becomes assertion-native |

Friction scale:

- `None`: less than 1 minute, pure mechanical rename
- `Low`: 1 to 5 minutes, simple rewrite
- `Medium`: 5 to 15 minutes, changed calling pattern or setup
- `High`: 15 to 30 minutes, notable rewrite or workaround
- `Blocking`: cannot migrate cleanly with the current harness

## Repeated rewrite patterns

- `extends GutTest` -> `extends GdTest`
- `yield_to(...)` -> `await assert_signal(...)` or `await wait_*`
- `double()` / `stub()` -> `mock()` / `stub()`
- `watch_signals(obj)` -> one `watch_signal(obj, sig)` per signal under test

## Blocking gaps

Record only the blockers that came up in the real suite:

- Native engine error interception (`assert_engine_error*`)
- Global signal watching on the whole object
- Project-specific helpers with no current `gdtest` equivalent
