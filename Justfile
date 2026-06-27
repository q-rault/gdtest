set dotenv-load
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Run everything: Rust unit tests, GDScript self-tests, CLI integration tests, docs examples.
test: test-rust build test-gd test-cli test-docs

# Rust unit tests (discovery, proto, etc.)
test-rust:
    cargo test --workspace --quiet

# Build release binary.
build:
    cargo build --release --quiet

# GDScript self-tests.
test-gd: build
    ./target/release/gdtest test/ --harness addons/gdtest/gdtest_worker.gd --reporter tap

# Shell-level CLI integration tests.
test-cli: build
    bash test/cli_tests.sh

# Docs examples - region-marked snippets in test/docs/ must pass.
test-docs: build
    ./target/release/gdtest test/docs/ --harness addons/gdtest/gdtest_worker.gd --reporter tap

# Benchmark gdtest vs GUT vs gdUnit4. Set BENCH_RUNS=1 for a quick smoke run.
bench: build
    PYTHONUTF8=1 PYTHON3="python" RUNS="${BENCH_RUNS:-3}" benchmarks/bench.sh --tool all --category all \
        | PYTHONUTF8=1 "python" benchmarks/report.py --save \
            --gdtest-bin ./target/release/gdtest \
            --godot-exe "$GODOT_EXE" \
            --runs "${BENCH_RUNS:-3}"

# Full CI pipeline.
ci: test bench

# Transpile test files between formats (GdTest <-> GUT <-> gdUnit4).
# Single file:
#   just transpile --target gut     test/cases/test_player.gd > out.gd   # GdTest -> GUT
#   just transpile --target gdunit4 test/cases/test_player.gd > out.gd   # GdTest -> gdUnit4
#   just transpile --source gut     test_player_gut.gd > out.gd           # GUT -> GdTest
#   just transpile --source gdunit4 test_player_gdunit4.gd > out.gd       # gdUnit4 -> GdTest
# Directory (--source rewrites files in place; use git to revert):
#   just transpile --source gut     test/cases/                           # migrate GUT dir -> GdTest
#   just transpile --source gdunit4 --check test/cases/                   # dry-run, exit 1 if diff
# just transpile --target gut     --output-dir /tmp/gut test/cases/     # GdTest dir -> GUT
transpile *ARGS:
    PYTHONUTF8=1 python benchmarks/transpile.py {{ ARGS }}

# Benchmark your own project's test suite against GUT and gdUnit4.
# Set GODOT_PROJECT_PATH and MANIFEST before running.
# Manifest format: one "path/to/test.gd [file|test]" line per file; isolation defaults to file.
# Example:
# GODOT_PROJECT_PATH=~/my-game MANIFEST=~/my-game/bench.manifest just bench-project
bench-project:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${GODOT_PROJECT_PATH:?Usage: GODOT_PROJECT_PATH=/path/to/game MANIFEST=... just bench-project}"
    : "${MANIFEST:?Usage: MANIFEST=/path/to/manifest.txt just bench-project}"
    PYTHONUTF8=1 PYTHON3="python" RUNS="${BENCH_RUNS:-3}" \
        benchmarks/bench.sh --tool all --manifest "$MANIFEST" \
        | PYTHONUTF8=1 "python" benchmarks/report.py

# Package the Godot runtime addon + editor plugin as a zip.
package-editor:
    #!/usr/bin/env bash
    version="$(cargo metadata --no-deps --format-version 1 | python -c "import sys, json; print(json.load(sys.stdin)['packages'][0]['version'])")"
    out="gdtest-editor-${version}.zip"
    if command -v zip &>/dev/null; then
        zip -r "$out" addons/gdtest/ addons/gdtest_editor/
    else
        powershell -Command "Compress-Archive -Path @('addons/gdtest','addons/gdtest_editor') -DestinationPath '$out' -Force"
    fi

# Build the VS Code extension (.vsix).
package-vscode:
    cd vscode-gdtest && npm ci --silent && npm run package

# Build the Zed extension WASM binary (requires: rustup target add wasm32-wasip1).
package-zed:
    cd zed-gdtest && cargo build --release --target wasm32-wasip1
    cp zed-gdtest/target/wasm32-wasip1/release/zed_gdtest.wasm zed-gdtest/extension.wasm

# Package all publishable artifacts.
package-all: package-editor package-vscode package-zed

check-clean-git:
    python scripts/release.py check-clean-git

check-release-version NEW_VER:
    python scripts/release.py check-release-version {{ NEW_VER }}

check-version-sync:
    python scripts/release.py check-version-sync

# Bump version across all manifests.
# Usage: just bump 0.2.0
bump NEW_VER:
    python scripts/release.py bump {{ NEW_VER }}

release NEW_VER:
    just check-clean-git
    just check-release-version "{{ NEW_VER }}"
    just bump "{{ NEW_VER }}"
    just check-version-sync
    cargo test --workspace --quiet
    git add Cargo.toml Cargo.lock vscode-gdtest/package.json vscode-gdtest/package-lock.json zed-gdtest/extension.toml addons/gdtest_editor/plugin.cfg Justfile PUBLISHING.md scripts/release.py
    git diff --cached --quiet; if ($LASTEXITCODE -eq 0) { Write-Output "release: no version file changes to commit; continuing" } else { git commit -m "release: v{{ NEW_VER }}" }
    git tag "v{{ NEW_VER }}"
    git push origin main
    git push origin "v{{ NEW_VER }}"

retag-release NEW_VER:
    just check-release-version "{{ NEW_VER }}"
    git tag -f "v{{ NEW_VER }}"
    git push origin --force-with-lease "v{{ NEW_VER }}"
