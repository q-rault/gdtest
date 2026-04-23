.PHONY: test test-gd test-cli test-rust test-docs test-external-project bench ci package-editor package-vscode package-zed package-all

VERSION := $(shell grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)

# Run everything: Rust unit tests, GDScript self-tests, CLI integration tests, docs examples,
# and optional external-project integration verification.
test: test-rust build test-gd test-cli test-docs

# Rust unit tests (discovery, proto, etc.)
test-rust:
	cargo test --workspace --quiet

# Build release binary (required by test-gd and test-cli).
build:
	cargo build --release --quiet

# GDScript self-tests — gdtest runs its own test suite through Godot.
# The gdtest repo keeps the worker at harness/ rather than addons/gdtest/.
test-gd: build
	./target/release/gdtest test/ --harness harness/gdtest_worker.gd --reporter tap

# Shell-level CLI integration tests.
test-cli: build
	bash test/cli_tests.sh

# Docs examples — every region-marked snippet in test/docs/ must pass.
# This is the source of truth for code shown in docs pages.
test-docs: build
	./target/release/gdtest test/docs/ --harness harness/gdtest_worker.gd --reporter tap

# Run an external project's test suite to verify compatibility.
# Usage: EXTERNAL_PROJECT_PATH=../my-project make test-external-project
test-external-project: build
	@test -n "$(EXTERNAL_PROJECT_PATH)" || (echo "set EXTERNAL_PROJECT_PATH=/path/to/project" && exit 1)
	./target/release/gdtest --project-path "$(EXTERNAL_PROJECT_PATH)" test/ --reporter tap

# Benchmark gdtest vs GUT vs gdUnit4.
# Set BENCH_RUNS=1 for a quick smoke run (CI), default is 3.
bench: build
	RUNS=$${BENCH_RUNS:-3} benchmarks/bench.sh --tool all --category all --save

# Full CI pipeline: tests + benchmarks.
ci: test bench

# --- Packaging ---

# Zip the Godot editor plugin for manual distribution.
package-editor:
	zip -r gdtest-editor-$(VERSION).zip addons/gdtest/ addons/gdtest_editor/

# Build the VS Code extension (.vsix).
package-vscode:
	cd vscode-gdtest && npm ci --silent && npm run package

# Build the Zed extension WASM binary. Requires the wasm32-wasip1 target:
#   rustup target add wasm32-wasip1
package-zed:
	cd zed-gdtest && cargo build --release --target wasm32-wasip1
	cp zed-gdtest/target/wasm32-wasip1/release/zed_gdtest.wasm zed-gdtest/extension.wasm

# Package all publishable artifacts.
package-all: package-editor package-vscode package-zed
