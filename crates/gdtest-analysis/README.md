# gdtest-analysis crate

`gdtest-analysis` is the library crate behind gdtest's GDScript analysis.

It provides:

- Discovery of `test_*` and `bench_*` functions, using the [gdsyntax](https://crates.io/crates/gdsyntax) GDScript parser
- Test-file walking and path pattern support
- Dependency and coverage-oriented source analysis
- Source rewriting used by GUT and gdUnit4 migration flows
- Mutation generation for mutation-testing runs

This crate is an internal dependency of `gdtest`, split out so the analysis layer stays isolated from the CLI runtime. It is published only because `gdtest` depends on it.

**Stability:** there is no API stability promise outside `gdtest`. Each `gdtest` release depends on the `gdtest-analysis` release with exactly the same version, and any version may change the API. If you depend on this crate directly, pin an exact version (`gdtest-analysis = "=X.Y.Z"`).

See the workspace [README](../../README.md) for end-user setup and CLI usage.
