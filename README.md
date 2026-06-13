# gdtest

Fast Godot 4 test runner with a Rust CLI and a small GDScript harness.

- Parallel workers with persistent Godot processes
- Discovery without starting Godot
- File or per-test isolation
- Watch mode, retries, sharding, coverage, HTML reports, mutation testing
- Migration helpers for GUT and gdUnit4

## Install

```sh
cargo install gdtest
```

Prebuilt binaries are attached to each [GitHub release](https://github.com/q-rault/gdtest/releases).

Optional integrations:

- Runtime harness: `gdtest init`
- Editor plugin: installed by `gdtest init`; opt out with `gdtest init --no-editor`
- VS Code: install the `vscode-gdtest-*.vsix` artifact from a release
- Zed: use the extension in `zed-gdtest/` or the registry package

More setup details live in the published docs: [Installation docs](https://gdtest.onrender.com/docs/installation).

## Project setup

Install the runtime addon into your Godot project:

```sh
gdtest init
```

This writes `addons/gdtest/` into the current project:

```text
your-project/
`-- addons/
    `-- gdtest/
        |-- gdtest_worker.gd
        |-- gdt.gd
        |-- gd_test.gd
        |-- gdunit4_compat.gd
        |-- gdunit4_chain.gd
        `-- scene_runner.gd
```

`gdtest init` also installs the Godot editor plugin by default. Skip it with:

```sh
gdtest init --no-editor
```

Enable **gdtest Editor** once in Godot at `Project -> Project Settings -> Plugins`.

On first run, gdtest may execute `godot --headless --import` once so Godot builds its script class cache.
If `--godot-exe`, `GODOT_EXE`, and `gdtest.toml` do not set a Godot binary, gdtest tries to discover a compatible Godot 4 executable automatically from common command names and common install locations.

## Quick start

Put test files under `test/` and extend `GdTest`:

```gdscript
extends GdTest

func test_arithmetic() -> void:
    assert_eq(1 + 1, 2)

func test_async() -> void:
    await wait_physics_frames(3)
    assert_true(Engine.get_physics_frames() > 0)
```

Run the suite:

```sh
gdtest test/
gdtest test/test_example.gd
gdtest test/test_example.gd::test_arithmetic
gdtest test/ --filter arithmetic
gdtest test/ --list
gdtest test/ -j 4
```

If the harness lives somewhere else, pass `--harness <path>`.

## CLI features

Common flags:

| Flag | Default | Effect |
| --- | --- | --- |
| `-j, --jobs <N>` | CPU count | Parallel worker count |
| `--reporter pretty\|tap\|json\|junit` | `pretty` | Output format |
| `--filter <REGEX>` | - | Match on `file :: func` |
| `--tag <TAG>` | - | Include tagged tests |
| `--exclude-tag <TAG>` | from config | Exclude tagged tests |
| `--list` | off | Print discovered tests and exit |
| `--isolation file\|test` | `file` | Reuse worker per file or respawn per test |
| `--timeout <MS>` | `5000` | Mark silent tests as errors |
| `--timeout-warn <MS>` | off | Warn on slow tests without failing them |
| `--retries <N>` | `0` | Retry failing tests |
| `--fail-fast` | off | Stop on first failure or error |
| `--watch` | off | Re-run on `.gd` file changes |
| `--partition count:N/M\|hash:N/M` | off | CI sharding |
| `--changed-only` / `--changed-since <REF>` | off | Run impacted tests only |
| `--coverage` | off | Write `coverage/lcov.info` |
| `--min-coverage <PCT>` | off | Fail below threshold |
| `--html-report [PATH]` | off | Write standalone HTML report |
| `--bench` | off | Run `bench_*` functions instead of `test_*` |
| `--mutate <FILE>` | off | Run mutation testing against one source file |
| `--update-snapshots` | off | Accept snapshot changes |

Subcommands:

```sh
gdtest aggregate shard-*.xml --output combined.xml
gdtest migrate gut test/ --out .gdtest_rewrite/gut
gdtest migrate gdunit4 test/ --out .gdtest_rewrite/gdunit4
```

## Assertion API

Two entry styles are supported.

`extends GdTest`:

```gdscript
extends GdTest

func test_example() -> void:
    assert_eq(a, b)
    assert_true(cond)
```

`Gdt.*` static calls for scripts that must extend another engine type:

```gdscript
extends Node

func test_example() -> void:
    Gdt.expect_eq(a, b)
    Gdt.expect_true(cond)
```

Common helpers:

```text
assert_eq / assert_ne
assert_lt / assert_lte / assert_gt / assert_gte
assert_almost_eq
assert_true / assert_false
assert_null / assert_not_null
assert_has / assert_not_has
assert_is / assert_between
assert_string_contains / assert_string_starts_with / assert_string_ends_with
assert_vec2_approx / assert_vec3_approx / assert_vec4_approx
assert_color_approx / assert_rect2_approx
assert_snapshot

await assert_signal / assert_no_signal / assert_signal_count / assert_signal_with_args

mock / stub / stub_with_args / get_calls
assert_called / assert_not_called / assert_call_count / assert_called_with

fail / pending / skip

await wait_physics_frames / wait_physics_seconds / wait_process_frame
add_child_autofree / instantiate_scene / scene_runner
forall
```

Lifecycle hooks:

```gdscript
func before_all() -> void: ...
func after_all() -> void: ...
func before_each() -> void: ...
func after_each() -> void: ...
```

Parameterized tests:

```gdscript
func get_test_add_cases() -> Array:
    return [
        ["zeroes", [0, 0, 0]],
        ["positive", [2, 3, 5]],
    ]

func test_add(a: int, b: int, expected: int) -> void:
    assert_eq(a + b, expected)
```

Describe blocks:

```gdscript
class describe_Movement:
    extends GdTest

    func test_moves_right() -> void: ...
    func test_moves_left() -> void: ...
```

## `gdtest.toml`

`gdtest.toml` is loaded from the project directory or a parent directory. Priority is:

```text
CLI flag > environment variable > gdtest.toml > built-in default
```

Example:

```toml
harness = "addons/gdtest/gdtest_worker.gd"
patterns = ["test/test_*.gd", "test/docs/**/test_*.gd"]
exclude_tags = ["harness-meta", "isolation-specific"]
coverage_exclude = ["addons/**", "test/**"]
plugins = ["res://addons/my_plugin/reporter.gd"]

[profile.ci]
reporter = "junit"
fail_fast = true
```

Use `--profile ci` to apply a named profile.

## Migration

GUT and gdUnit4 migrations write converted copies and reports without touching the source files:

```sh
gdtest migrate gut test/ --out .gdtest_rewrite/gut
gdtest migrate gdunit4 test/ --out .gdtest_rewrite/gdunit4
```

Most simple GUT ports are mechanical:

| GUT | GdTest |
| --- | --- |
| `extends GutTest` | `extends GdTest` |
| `await get_tree().process_frame` | `await wait_process_frame()` |
| `await get_tree().create_timer(t).timeout` | `await wait_physics_seconds(t)` |
| `gut.p("...")` / `gut_print(...)` | `print(...)` |

## Benchmarks

Benchmark tooling lives in [benchmarks/README.md](benchmarks/README.md). It can compare gdtest against GUT and gdUnit4 on any local Godot project through `GODOT_PROJECT_PATH`.

## Workspace

- `crates/gdtest/`: CLI runner
- `crates/gdtest-analysis/`: GDScript discovery and migration library
- `addons/gdtest/`: runtime harness shipped into Godot projects
- `addons/gdtest_editor/`: optional editor UI
- `benchmarks/`: benchmark and evaluation scripts

## License

Dual licensed under MIT or Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
