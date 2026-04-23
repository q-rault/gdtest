# [Game Name] - gdtest Evaluation Scorecard

- Repository: `https://github.com/...`
- Source framework: `GUT` / `gdUnit4`
- Evaluation date: `YYYY-MM-DD`
- Evaluator: `name`

| Axis | Score /5 | Evidence | Notes |
| --- | --- | --- | --- |
| A. Setup friction |  | Install time, manual steps, blockers |  |
| B. API coverage |  | Covered assertions / total used |  |
| C. Migration effort |  | Lines changed / total test LOC |  |
| D. Failure quality |  | File, line, diff, assertion context |  |
| E. Performance |  | `gdtest` time vs source runner |  |
| F. CI readiness |  | Exit code, headless run, JUnit XML |  |
| G. Advanced features |  | Mocking, signals, async, scenes |  |

**Total score:** `__/35`  
**Verdict:** `Drop-in` / `Light migration` / `Heavy migration` / `Incompatible`

## Scoring rubric

| Axis | 5 | 4 | 3 | 2 | 1 |
| --- | --- | --- | --- | --- | --- |
| A. Setup friction | `< 2 min`, no errors | `< 5 min` | `< 10 min` | `< 20 min` | `>= 20 min` or blocked |
| B. API coverage | `100%` | `>= 90%` | `>= 75%` | `>= 50%` | `< 50%` |
| C. Migration effort | `0-2%` lines changed | `2-5%` | `5-15%` | `15-30%` | `> 30%` |
| D. Failure quality | message + file + line + diff | 3 elements | 2 elements | 1 element | unusable |
| E. Performance | `<= 1.0x` source time | `<= 1.2x` | `<= 1.5x` | `<= 2.0x` | `> 2.0x` |
| F. CI readiness | exit code + headless + XML | exit code + headless | exit code only | partial | blocked |
| G. Advanced features | 4/4 areas covered | 3/4 | 2/4 | 1/4 | 0/4 |

## Verdict guide

| Total | Verdict | Meaning |
| --- | --- | --- |
| `30-35` | Drop-in | Migration is low risk and mostly mechanical. |
| `24-29` | Light migration | A few local rewrites or workflow changes are needed. |
| `16-23` | Heavy migration | Port is viable, but cost is significant. |
| `0-15` | Incompatible | Critical gaps or blockers remain. |

## Evidence checklist

- Link the benchmark output or paste the command summary.
- Record the assertion gaps that prevented direct migration.
- Capture at least one representative failing test output for axis D.
- Note whether the project needed `--isolation=test` for correctness.
