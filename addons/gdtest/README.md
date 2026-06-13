# gdtest Runtime Addon — version 0.1.0

Copy this folder into your Godot project as `addons/gdtest/`. Use the release archive that **matches your `gdtest` CLI version** (requires CLI 0.1.x, Godot 4.0+).

This addon is the runtime side of gdtest. The Rust CLI discovers tests and manages workers; the scripts in this folder execute those tests inside Godot.

Files included:

- `gdtest_worker.gd`: worker entrypoint used by the `gdtest` CLI
- `gdt.gd`: assertion, mocking, async, snapshot, and property-test helpers (`class_name Gdt`)
- `gd_test.gd`: optional base class for instance-style tests (`class_name GdTest`)
- `scene_runner.gd`: helper for scene-driven tests
- `gdunit4_compat.gd` and `gdunit4_chain.gd`: compatibility helpers used by gdUnit4 migrations

Typical CLI usage:

```sh
gdtest test/ --harness addons/gdtest/gdtest_worker.gd
```

Typical test file:

```gdscript
extends GdTest

func test_player_takes_damage() -> void:
    assert_eq(health.current, 10)
```

You can also call `Gdt.*` statically from scripts that already extend another engine type.
