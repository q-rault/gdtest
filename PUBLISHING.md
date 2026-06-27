# Publishing

`gdtest` uses a source-first, manual release flow. The committed repo
state is the release source of truth:

- `Cargo.toml`
- aligned version-bearing extension/plugin manifests updated by `just bump`
- a matching Git tag `v<version>`

The GitHub Actions workflow at
[`.github/workflows/release.yml`](.github/workflows/release.yml)
builds and publishes artifacts from the pushed tag. It does not modify
tracked version files or create tags.

## One-time setup

1. Push the repo to GitHub at `https://github.com/q-rault/gdtest` (or wherever matches `repository` in `Cargo.toml`).
2. Create a [crates.io API token](https://crates.io/me) scoped to `publish-new` and `publish-update`, then store it as the `CARGO_REGISTRY_TOKEN` repository secret.
3. Confirm `gdtest` is still available on crates.io:
   ```sh
   curl -sI https://crates.io/api/v1/crates/gdtest | grep -E '^HTTP'
   ```
   `404` means available.
4. Verify `cargo package -p gdtest --allow-dirty` runs cleanly from `main`.

## Releasing

```sh
just release 0.2.0
just release 0.2.0-beta.1
```

`just release` will:

1. verify the git worktree is clean
2. validate the version string
3. update all version-bearing files
4. run `cargo test --workspace --quiet`
5. create commit `release: v<version>`
6. create tag `v<version>`
7. push `main` and the tag

The workflow then:

1. Builds release binaries on macOS arm64 + x86_64, Linux x86_64, Windows x86_64.
2. Bundles `gdtest`, `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `addons/gdtest/`, and `addons/gdtest_editor/`.
3. Creates a GitHub release with the archives attached.

After the workflow finishes, verify `cargo install gdtest` still works and update any install docs if the artifact naming changes.

## Failed release recovery

If the workflow fails but the tag already points at the intended
release commit, prefer fixing the workflow and re-running the job.

If the fix requires a new commit and the original release was not
successfully published, move the same tag instead of creating a new
patch version only for operational recovery:

```sh
just retag-release 0.2.0
```

Equivalent raw git commands:

```sh
git tag -f v0.2.0
git push origin -f v0.2.0
```

## Rollback

crates.io versions are immutable. To withdraw a broken release:

```sh
cargo yank --version 0.1.0 -p gdtest
```

`cargo install gdtest` will then refuse the yanked version, while still
allowing an explicit `cargo install gdtest@0.1.0` for recovery.
