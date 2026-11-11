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

# Full CI pipeline.
ci: test

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

# Copy addons/ into the gdtest crate, which embeds them for `gdtest init`.
sync-assets:
    python scripts/release.py sync-assets
