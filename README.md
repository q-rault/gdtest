# gdtest

Fast test runner for Godot 4 written in Rust.

- Parallel workers (`-j N`)
- Persistent Godot workers to reduce startup cost
- Crash recovery when a test kills the worker
- Rust-based discovery with no Godot process needed

## Install

```sh
cargo install gdtest
```

Prebuilt binaries are attached to each [GitHub release](https://github.com/q-rault/gdtest/releases).

Optional integrations:

- Runtime harness: copy `addons/gdtest/` into your project
- Godot editor plugin: copy `addons/gdtest_editor/` into your project and enable it in Project Settings -> Plugins
- VS Code: install `vscode-gdtest-*.vsix` from a release
- Zed: use the registry entry or `zed-gdtest/` from this repo

More setup details: [Installation docs](https://q-rault.github.io/gdtest/docs/installation)

## Setup

Copy these files into your Godot project:

```text
your-project/
`-- addons/
    `-- gdtest/
        |-- gdtest_worker.gd
        |-- gdt.gd
        `-- gd_test.gd
```

On first run, gdtest may execute `godot --headless --import` once to build Godot's class cache.

## Write a test

Put test files under `test/`:

```gdscript
extends GdTest

func test_arithmetic() -> void:
    assert_eq(1 + 1, 2)

func test_async() -> void:
    await wait_physics_frames(3)
    assert_true(Engine.get_physics_frames() > 0)
```

Use `extends GdTest` for instance-style assertions. Each test runs in a fresh instance.

## Run

```sh
gdtest test/
gdtest test/test_example.gd
gdtest test/ --filter "arithmetic"
gdtest test/ --list
gdtest test/ --reporter tap
gdtest test/ -j 4
```

If the harness lives somewhere else, pass `--harness <path>`.

## Common options

| Flag | Default | Effect |
| --- | --- | --- |
| `-j, --jobs <N>` | `num_cpus` | Parallel worker count |
| `--reporter pretty\|tap\|junit` | `pretty` | Output format |
| `--filter <REGEX>` | - | Run matching tests only |
| `--list` | - | Print discovered tests and exit |
| `--isolation file\|test` | `file` | Reuse worker per file or respawn per test |
| `--timeout <MS>` | `5000` | Mark silent tests as errors after this timeout |
| `--watch` | off | Re-run on `.gd` file changes |
| `--no-auto-import` | off | Skip the one-time `godot --headless --import` check |
| `--godot-exe <PATH>` | `$GODOT_EXE` | Godot binary |
| `--project-path <DIR>` | `.` | Godot project root |
| `--harness <PATH>` | `addons/gdtest/gdtest_worker.gd` | Worker harness path |

## Assertion API

Two styles:

`extends GdTest`:

```gdscript
extends GdTest

func test_example() -> void:
    assert_eq(a, b)
    assert_true(cond)
```

`Gdt.*` static calls:

```gdscript
extends Node

func test_example() -> void:
    Gdt.expect_eq(a, b)
    Gdt.expect_true(cond)
```

Use `Gdt.*` when the test must already extend an engine type.

Common helpers:

```text
assert_eq / assert_ne
assert_lt / assert_lte / assert_gt / assert_gte
assert_almost_eq(actual, expected, tolerance = 1e-6)
assert_true / assert_false
assert_null / assert_not_null
assert_has / assert_not_has
assert_is / assert_between
assert_string_contains / assert_string_starts_with / assert_string_ends_with
assert_vec2_approx / assert_vec3_approx / assert_vec4_approx
assert_color_approx / assert_rect2_approx

await assert_signal(node, "signal_name", timeout_ms = 1000)
await assert_no_signal(node, "signal_name", timeout_ms = 200)
await assert_signal_count(node, "signal_name", n, timeout_ms = 1000)
await assert_signal_with_args(node, "signal_name", [args], timeout_ms = 1000)

mock(MyClass)
stub(mock_obj, "method", return_value)
stub_with_args(mock_obj, "method", [args], return_value)
get_calls(mock_obj, "method")
assert_called / assert_not_called / assert_call_count / assert_called_with

fail(msg)
pending(msg)
skip(msg)

await wait_physics_frames(n = 1)
await wait_physics_seconds(s)
await wait_process_frame()

add_child_autofree(node)
instantiate_scene(packed)
scene_runner(packed)
```

### Lifecycle hooks

```gdscript
func before_all() -> void: ...
func after_all() -> void: ...
func before_each() -> void: ...
func after_each() -> void: ...
```


### Parameterized tests

```gdscript
func get_test_add_cases() -> Array:
    return [
        ["zeroes", [0, 0, 0]],
        ["positive", [2, 3, 5]],
    ]

func test_add(a: int, b: int, expected: int) -> void:
    assert_eq(a + b, expected)
```

### Describe blocks

```gdscript
class describe_Movement:
    extends GdTest

    func test_moves_right() -> void: ...
    func test_moves_left() -> void: ...
```

## Migrating from GUT

Most tests only need a header change:

| GUT | GdTest |
| --- | --- |
| `extends GutTest` | `extends GdTest` |
| `await get_tree().process_frame` | `await wait_process_frame()` |
| `await get_tree().create_timer(t).timeout` | `await wait_physics_seconds(t)` |
| `gut.p("...")` / `gut_print(...)` | `print(...)` |

Most assertion names and lifecycle hooks are already compatible.

For larger migrations:

```sh
gdtest migrate gut test/ --out .gdtest_rewrite/gut
gdtest migrate gdunit4 test/ --out .gdtest_rewrite/gdunit4
```

Both commands emit rewritten copies plus migration reports.

Docs: [https://gdtest.dev/docs](https://gdtest.dev/docs)

## Benchmarks

See [benchmarks/README.md](benchmarks/README.md). Benchmark scripts target any local Godot project via `GODOT_PROJECT_PATH`; game-specific outputs should stay untracked.

## License

Dual licensed under MIT or Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
