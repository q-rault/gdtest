# gdtest Runtime Addon

Copy this folder into your Godot project as `addons/gdtest/`.

Files included:

- `gdtest_worker.gd` - worker entrypoint used by the `gdtest` CLI
- `gdt.gd` - assertion and async helper API (`class_name Gdt`)
- `gd_test.gd` - optional base class for instance-style tests (`class_name GdTest`)
- support files used by the runtime harness

Typical CLI usage:

```sh
gdtest test/ --harness addons/gdtest/gdtest_worker.gd
```
