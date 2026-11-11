# Publishing

`gdtest` releases are tag-driven. Pushing a `v<version>` tag starts **Publish release** (`.github/workflows/release.yml`), which checks that the tag matches the version in `Cargo.toml`, builds the archives (the binary plus `addons/gdtest/` and `addons/gdtest_editor/`), publishes gdtest-analysis and gdtest to crates.io (trusted publishing, no token), then creates the GitHub release. Tags with a prerelease suffix (`v1.3.0-rc.1`) become GitHub prereleases and are not sent to gdsuite.

The version source of truth is the committed repo state: `Cargo.toml` (with the `=<version>` pin on gdtest-analysis), `Cargo.lock`, the addon and editor manifests, the VS Code and Zed extensions, and the addons copied into `crates/gdtest/assets`. `scripts/release.py bump` updates the version and `cargo update --workspace` the lockfile.

## Release from GitHub

**Actions → Cut release → Run workflow**, on `main`:

| Input | Values |
| --- | --- |
| bump | `patch`, `minor` or `major` |
| pre | `none` for a stable release, or `alpha`, `beta`, `rc` |
| version | an exact version instead, such as `1.3.0-rc.1` (overrides the two above) |
| dry run | pick the version and run the tests, push nothing |

It bumps the version, runs `cargo test --workspace --locked` (the Godot self-tests run in CI on main, not here), commits `release: v<version>` to `main`, pushes the tag and starts Publish release and `docs.yml`. The run summary shows the version it picked.

How the version is picked, from the current one:

| Current | bump | pre | Next |
| --- | --- | --- | --- |
| 1.2.3 | patch | none | 1.2.4 |
| 1.2.3 | minor | none | 1.3.0 |
| 1.2.3 | major | none | 2.0.0 |
| 1.2.3 | minor | beta | 1.3.0-beta.1 |
| 1.3.0-beta.1 | minor | beta | 1.3.0-beta.2 |
| 1.3.0-beta.2 | minor | rc | 1.3.0-rc.1 |
| 1.3.0-rc.1 | minor | none | 1.3.0 |
| 1.3.0-rc.1 | major | none | 2.0.0 |

From a prerelease, a bump that already leads to its version stays on it, so keep the same `bump` through the betas, the release candidates and the release. Going back (from `rc` to `beta`) is refused. Check a pick without releasing with `python scripts/release.py next-version minor rc`.

## Release from a checkout

From a clean `main` checkout:

```sh
just release 0.2.0
just release 0.2.0-beta.1
```

`just release` updates the version files, runs the tests, commits `release: v<version>`, tags it and pushes `main` and the tag.

## Failed release recovery

If Publish release fails but the tag already points to the correct release commit, prefer fixing the workflow and re-running the job.

If the fix requires a new commit and the original release did not publish to crates.io, move the same tag to the new commit instead of inventing a patch version. Pushing the moved tag starts Publish release again:

```sh
just retag-release 0.2.0
```

Once crates.io has a version, it cannot be published again: release the next patch instead.

## Rollback

crates.io versions are immutable. To withdraw a broken release:

```sh
cargo yank --version 0.1.0 -p gdtest
```

`cargo install gdtest` will then refuse the yanked version, while still
allowing an explicit `cargo install gdtest@0.1.0` for recovery.

The docs site is published from release and prerelease tags; see [docs/README.md](docs/README.md#published-versions).
