# gdtest-analysis crate

`gdtest-analysis` is the library crate behind gdtest's GDScript analysis.

It provides:

- Tree-sitter-based discovery of `test_*` and `bench_*` functions
- Test-file walking and path pattern support
- Dependency and coverage-oriented source analysis
- Source rewriting used by GUT and gdUnit4 migration flows
- Mutation generation for mutation-testing runs

This crate is primarily an internal workspace dependency of `gdtest`, but it is split out so the analysis layer stays isolated from the CLI runtime concerns.

See the workspace [README](../../README.md) for end-user setup and CLI usage.
