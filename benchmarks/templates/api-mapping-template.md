# [Game Name] - API Mapping

Source framework: `GUT` / `gdUnit4`

| Framework | Source API | gdtest equivalent | Status | Notes / workaround |
| --- | --- | --- | --- | --- |
| GUT | `assert_eq(a, b)` | `assert_eq(a, b)` | Direct |  |
| GUT | `assert_has(collection, item)` | `assert_contains(collection, item)` | Partial | Same intent, different name. |
| GUT | `assert_between(v, lo, hi)` | `assert_gt(v, lo)` + `assert_lt(v, hi)` | Gap | Two-call workaround. |
| GUT | `yield_to(signal, timeout)` | `await wait_until(func(): ...)` or signal helpers | Partial | Depends on the original semantics. |
| GUT | `gut.double(MyClass)` | `mock(MyClass)` | Partial | Verification API differs. |
| gdUnit4 | `assert_that(v).is_equal(expected)` | `assert_eq(v, expected)` | Partial | Fluent chain becomes positional assertion. |
| gdUnit4 | `mock(Type)` | `mock(Type)` | Direct |  |
| gdUnit4 | `auto_free(node)` | `add_child_autofree(node)` | Partial | Similar cleanup intent. |
| gdUnit4 | `InputFactory.simulate_key_pressed(k)` | `SceneRunner.simulate_key_pressed(k)` | Partial | Runner-based integration flow. |

## Status legend

- `Direct`: same intent and nearly identical call shape
- `Partial`: simple rewrite, renamed API, or changed calling pattern
- `Gap`: no direct equivalent; requires workaround or unsupported behavior

## Project-specific additions

Add one row per distinct assertion/helper used by the source suite. When multiple
tests repeat the same pattern, keep one row and point the Notes column at the
affected files.
