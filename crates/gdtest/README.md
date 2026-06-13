# gdtest crate

`gdtest` is the CLI crate for the gdtest workspace.

It provides:

- Rust-side test discovery and scheduling
- Parallel Godot worker management
- Reporters: `pretty`, `tap`, `json`, `junit`
- Watch mode, retries, sharding, coverage, HTML reports
- Migration commands for GUT and gdUnit4
- Mutation testing and changed-test selection

Install from crates.io:

```sh
cargo install gdtest
```

Minimal project setup:

1. Copy `addons/gdtest/` into your Godot project.
2. Put tests under `test/`.
3. Run `gdtest test/`.

Example:

```sh
gdtest test/ -j 4
gdtest test/ --reporter junit
gdtest test/ --coverage --html-report
gdtest migrate gut test/
```

The full project documentation lives in the workspace [README](../../README.md).
