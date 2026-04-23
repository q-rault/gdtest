# Publishing checklist

How to ship a new version of gdtest. The GitHub Actions workflow at
[`.github/workflows/release.yml`](.github/workflows/release.yml) does
the heavy lifting on tag push; this document is the small list of
things to do once before the first release.

## One-time setup

1. **Push the repo to GitHub** at `https://github.com/q-rault/gdtest`
   (or wherever — match `repository` in `Cargo.toml`).
2. **Create a [crates.io API token](https://crates.io/me)** scoped to
   `publish-new` and `publish-update`. Add it as a repo secret named
   `CARGO_REGISTRY_TOKEN` under `Settings → Secrets and variables →
   Actions`.
3. **Confirm `gdtest` is still free on crates.io.** Run:
   ```sh
   curl -sI https://crates.io/api/v1/crates/gdtest | grep -E '^HTTP'
   ```
   A `404` means available. (Verified free 2026-04-24.)
4. **Verify `cargo package -p gdtest --allow-dirty`** runs cleanly
   from main.

## Releasing

```sh
# bump versions in Cargo.toml workspace.package
git commit -am "release: v0.1.0"
git tag v0.1.0
git push origin main --tags
```

The workflow then:

1. Builds release binaries on macOS arm64 + x86_64, Linux x86_64,
   Windows x86_64. Each archive bundles `gdtest`, `README.md`,
   `LICENSE-MIT`, `LICENSE-APACHE`, and `addons/gdtest/` (so users who download the binary
   get the runtime addon drop-in alongside).
2. Creates a GitHub release with all archives attached + auto-generated
   notes.
3. Publishes the three crates to crates.io in dependency order:
   `gdtest-proto` → `gdtest-discovery` → `gdtest`.

After the workflow finishes:

- `cargo install gdtest` should work. Verify on a fresh machine.
- Update the README's install section if the binary URL pattern
  changes.

## Manual dry run (without publishing)

The workflow accepts `workflow_dispatch` with a `tag` input — it
builds and uploads to the release page but **skips** the crates.io
publish (the `if: ${{ !github.event.inputs.tag }}` guard). Use this to
test the binary build pipeline without burning a crates.io version.

## Rollback

crates.io versions are immutable. To withdraw a broken release:

```sh
cargo yank --version 0.1.0 -p gdtest
```

`cargo install gdtest` will then refuse the yanked version (still
available for explicit `cargo install gdtest@0.1.0` as a recovery
escape).
