# gdtest Editor Plugin

Copy this folder into your Godot project as `addons/gdtest_editor/`, then enable it in `Project Settings -> Plugins`.

This plugin is optional. It provides an editor-side test panel; the actual runner and test API still live in `addons/gdtest/` and the `gdtest` CLI.

Use this plugin together with:

- `addons/gdtest/` for the runtime harness
- the `gdtest` binary for CLI runs, watch mode, coverage, migration, and CI

The panel resolves the `gdtest` binary in this order:

- `GDTEST_BIN`
- nearby project-local binaries
- `gdtest` on `PATH`
