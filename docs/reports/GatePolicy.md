# Quality Gate Policy

**Version**: 2.1
**Status**: Active
**Date**: April 2026
**Source**: `.github/workflows/ci.yml`, `docs/TechSpec.md`

---

## 1. Overview

This document defines the current quality gates for Kraken TUI release readiness. Some gates are blocking in CI, while others are reporting-only guardrails that still require human review. Additional local verification commands are listed where the source tree already provides them but CI does not yet treat them as blocking.

For reporting-only benchmark jobs, the benchmark commands themselves are still expected to compile and run successfully in CI. "Reporting-only" means the job does not currently parse the emitted numbers into an automatic regression threshold failure.

Current CI executes the host test and benchmark surfaces on `ubuntu-latest`. Cross-platform release artifacts are built in the release workflow, but the full host verification matrix is not yet exercised on macOS and Windows in CI.

Repo-side host verification entrypoints that `dlopen` directly are expected to validate the local Cargo-built artifact rather than a staged prebuild, so branch verification is tied to the code under review.

---

## 2. Gate Definitions

### Gate 1: Native Tests (including Golden Snapshots)

| Property | Value |
| --- | --- |
| **Metric** | All native tests pass and golden fixtures match committed snapshots |
| **Threshold** | 0 failures |
| **CI Job** | `native-tests` |
| **Enforcement** | `cargo test --manifest-path native/Cargo.toml` |
| **Recovery** | Fix the failure. If render output changed intentionally, regenerate fixtures with `GOLDEN_UPDATE=1 cargo test --manifest-path native/Cargo.toml golden`. |

### Gate 2: Native Code Quality (Clippy + Fmt)

| Property | Value |
| --- | --- |
| **Metric** | Zero clippy warnings and formatting consistency |
| **Threshold** | 0 warnings with `-D warnings` |
| **CI Job** | `native-tests` |
| **Enforcement** | `cargo clippy --manifest-path native/Cargo.toml -- -D warnings` and `cargo fmt --manifest-path native/Cargo.toml -- --check` |

### Gate 3: Native Benchmark Regression

| Property | Value |
| --- | --- |
| **Metric** | Writer, text-cache, and substrate benchmark health |
| **Threshold** | No materially suspicious regression in the benchmark output for tracked suites |
| **CI Job** | `native-benchmarks` |
| **CI Mode** | Reporting-only |
| **Enforcement** | `cargo bench --manifest-path native/Cargo.toml --bench writer_bench`, `cargo bench --manifest-path native/Cargo.toml --bench text_cache_bench`, and `cargo bench --manifest-path native/Cargo.toml --bench text_substrate_bench` must complete successfully in CI; the reported numbers are not yet parsed into an automatic regression threshold failure |
| **Tracked suites** | `writer_compact_*`, `writer_emit_*`, `writer_pipeline_full`, `cache_insert_1000`, `cache_get_hit_1000`, `cache_eviction_pressure`, `substrate_append_*`, `substrate_set_cursor_*`, `substrate_byte_to_visual_*` |
| **Local supplementary check** | `cargo bench --manifest-path native/Cargo.toml --bench devtools_bench` |

### Gate 4: Writer Throughput Reduction

| Property | Value |
| --- | --- |
| **Metric** | Style and cursor operation reduction versus per-cell emission baseline |
| **Threshold** | `>= 35%` reduction for the tracked writer tests |
| **CI Job** | `native-tests` |
| **Enforcement** | Native unit assertions such as `writer_reduces_ops_*` |

### Gate 5: Host Test Surface

| Property | Value |
| --- | --- |
| **Metric** | Host integration, reconciler, example replay, install smoke, and runner API tests all pass |
| **Threshold** | 0 failures |
| **CI Job** | `host-tests` |
| **Enforcement** | `bun test ts/test-ffi.test.ts`, `bun test ts/test-jsx.test.ts`, `bun test ts/test-examples.test.ts`, `bun test ts/test-install.test.ts`, `bun test ts/test-runner.test.ts` |

### Gate 6: Host Bundle Budget

| Property | Value |
| --- | --- |
| **Metric** | Minified TypeScript bundle size |
| **Threshold** | `< 75KB` |
| **CI Job** | `host-tests` |
| **Enforcement** | `bun run ts/check-bundle.ts` |

### Gate 7: Host Render Performance

| Property | Value |
| --- | --- |
| **Metric** | Host-side render frame time, mutation cycle time, and render-duration counters |
| **Threshold** | Remain within the intended `< 16ms` interactive envelope |
| **CI Job** | `host-benchmarks` |
| **CI Mode** | Blocking |
| **Enforcement** | `bun run ts/bench-render.ts` |

### Gate 8: FFI Call Overhead

| Property | Value |
| --- | --- |
| **Metric** | Single FFI call round-trip latency |
| **Threshold** | `< 1ms` |
| **CI Job** | `host-benchmarks` |
| **CI Mode** | Blocking |
| **Enforcement** | `bun run ts/bench-ffi.ts` |

---

## 3. Gate Flow

```text
CI Trigger
  |
  |-- native-tests ------ native tests + fmt + clippy + writer assertions
  |-- native-benchmarks - writer and text-cache benchmark reporting
  |-- host-tests -------- ffi + jsx + examples + install + runner + bundle
  |-- host-benchmarks --- ffi bench + render bench (blocking)
  `-- quality-gate ------ aggregate pass/fail result
```

---

## 4. Gate-to-Contract Mapping

| Gate | Primary Contract | Source of Truth |
| --- | --- | --- |
| Gate 1 | Render and replay correctness | `docs/TechSpec.md`, `native/fixtures/`, native test suite |
| Gate 2 | Native implementation hygiene | `docs/TechSpec.md`, Rust lint/format rules |
| Gate 3 | Writer, text-cache, and substrate throughput health | `docs/TechSpec.md`, native benches |
| Gate 4 | Terminal emission efficiency | `docs/TechSpec.md`, writer tests |
| Gate 5 | Host wrapper and example correctness | `docs/TechSpec.md`, `ts/` test suite |
| Gate 6 | Host bundle-size constraint | `docs/PRD.md`, `docs/TechSpec.md`, `ts/check-bundle.ts` |
| Gate 7 | Interactive render budget target | `docs/PRD.md`, `docs/TechSpec.md`, render bench |
| Gate 8 | Foreign-function overhead target | `docs/PRD.md`, `docs/TechSpec.md`, FFI bench |

---

## 5. Fixture Update Protocol

When intentional render changes cause golden mismatches:

1. Confirm the change is intentional.
2. Regenerate the fixtures with `GOLDEN_UPDATE=1 cargo test --manifest-path native/Cargo.toml golden`.
3. Review the updated fixture output.
4. Commit the fixture changes alongside the code change.

---

## 6. Benchmark Baseline Notes

- Criterion baselines are machine-local and are not committed to git.
- `native-benchmarks` is currently a reporting-only guardrail job for the tracked Criterion suites.
- `host-benchmarks` is currently blocking for the scripted FFI and render budget checks.
- `devtools_bench` exists in the source tree and should be used during devtools-sensitive changes even though it is not currently a blocking CI step.
