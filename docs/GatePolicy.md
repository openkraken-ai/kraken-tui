# v3 Quality Gate Policy

**Version**: 1.0
**Status**: Active
**Date**: March 2026
**Source**: ADR-T30, TASK-G4

---

## 1. Overview

This document defines the blocking quality gates for v3 release readiness. All gates are enforced in CI via `.github/workflows/ci.yml`. A release build must pass every gate to proceed.

---

## 2. Gate Definitions

### Gate 1: Native Tests (including Golden Snapshots)

| Property | Value |
|----------|-------|
| **Owner** | TASK-G1 |
| **Metric** | All `cargo test` pass, golden fixtures match committed snapshots |
| **Threshold** | 0 failures |
| **CI Job** | `native-tests` |
| **Enforcement** | `cargo test --manifest-path native/Cargo.toml` |
| **Recovery** | Fix test failures. Regenerate fixtures with `GOLDEN_UPDATE=1 cargo test golden` if intentional render changes. |

### Gate 2: Native Code Quality (Clippy + Fmt)

| Property | Value |
|----------|-------|
| **Owner** | TASK-G4 |
| **Metric** | Zero clippy warnings, formatting consistent |
| **Threshold** | 0 warnings (`-D warnings`) |
| **CI Job** | `native-tests` |
| **Enforcement** | `cargo clippy -- -D warnings` and `cargo fmt -- --check` |

### Gate 3: Native Benchmark Regression

| Property | Value |
|----------|-------|
| **Owner** | TASK-G2 |
| **Metric** | Writer throughput and text cache performance |
| **Threshold** | No regression beyond Criterion's default statistical significance (p < 0.05) |
| **CI Job** | `native-benchmarks` |
| **Enforcement** | `cargo bench --bench writer_bench` and `cargo bench --bench text_cache_bench` |
| **Benchmark Suites** | |
| - `writer_compact_{full,medium,sparse}` | Run compaction throughput at 3 density levels |
| - `writer_emit_{full,medium,sparse}` | Terminal emission throughput at 3 density levels |
| - `writer_pipeline_full` | End-to-end compact + emit pipeline |
| - `cache_insert_1000` | Cache insertion throughput |
| - `cache_get_hit_1000` | Cache lookup throughput |
| - `cache_eviction_pressure` | Eviction behavior under memory pressure |

### Gate 4: Writer Throughput Reduction

| Property | Value |
|----------|-------|
| **Owner** | TASK-A3, TASK-G2 |
| **Metric** | Style + cursor ops reduction vs per-cell baseline |
| **Threshold** | >= 35% reduction (TechSpec §5.5) |
| **CI Job** | `native-tests` (enforced via unit test assertions) |
| **Tests** | `writer_reduces_ops_full_diff`, `writer_reduces_ops_medium_diff`, `writer_reduces_ops_sparse_diff` |

### Gate 5: Host FFI Integration Tests

| Property | Value |
|----------|-------|
| **Owner** | TASK-G3 |
| **Metric** | All FFI integration tests and JSX reconciler tests pass |
| **Threshold** | 0 failures |
| **CI Job** | `host-tests` |
| **Enforcement** | `bun test ts/test-ffi.test.ts` and `bun test ts/test-jsx.test.ts` |

### Gate 6: Host Bundle Budget

| Property | Value |
|----------|-------|
| **Owner** | TASK-G4 |
| **Metric** | Minified TypeScript bundle size |
| **Threshold** | < 75 KB (PRD §5) |
| **CI Job** | `host-tests` |
| **Enforcement** | `bun run ts/check-bundle.ts` |

### Gate 7: Host Render Performance

| Property | Value |
|----------|-------|
| **Owner** | TASK-G3 |
| **Metric** | Host-side render frame time, mutation cycle time, render duration counter |
| **Thresholds** | |
| - Render frame | < 16 ms (60fps budget, PRD §5) |
| - Mutation + render cycle | < 16 ms |
| - Render duration (perf counter) | < 16 ms |
| **CI Job** | `host-benchmarks` |
| **Enforcement** | `bun run ts/bench-render.ts` (exits non-zero on gate failure) |

### Gate 8: FFI Call Overhead

| Property | Value |
|----------|-------|
| **Owner** | TASK-G3 |
| **Metric** | Single FFI call round-trip latency |
| **Threshold** | < 1 ms (< 1000 μs, PRD §5) |
| **CI Job** | `host-benchmarks` |
| **Enforcement** | `bun run ts/bench-ffi.ts` |

---

## 3. Gate Flow

```
CI Trigger (push/PR)
    │
    ├── native-tests ─────────── Gates 1, 2, 4
    │
    ├── native-benchmarks ────── Gate 3
    │
    ├── host-tests ───────────── Gates 5, 6
    │
    ├── host-benchmarks ──────── Gates 7, 8
    │
    └── quality-gate ─────────── All gates pass → release ready
```

---

## 4. Gate-to-Ticket Mapping

| Gate | PRD Constraint | TechSpec Section | Owning Epic/Task |
|------|---------------|------------------|-----------------|
| 1 | Contributor Experience | §5.5 | Epic G / TASK-G1 |
| 2 | API Stability | §5.3 | Epic G / TASK-G4 |
| 3 | Render Budget | §5.5 | Epic G / TASK-G2 |
| 4 | Render Budget | §5.5 (writer >= 35%) | Epic A / TASK-A3 |
| 5 | API Stability | §4 | Epic D-F / various |
| 6 | Host Bundle < 75KB | §5.5 | Epic G / TASK-G4 |
| 7 | Render < 16ms | §5.5 | Epic G / TASK-G3 |
| 8 | FFI < 1ms | §5.5 | Epic G / TASK-G3 |

---

## 5. Fixture Update Protocol

When intentional render changes cause golden snapshot mismatches:

1. Verify the change is intentional (review diff output from failing test).
2. Run `GOLDEN_UPDATE=1 cargo test --manifest-path native/Cargo.toml golden` to regenerate fixtures.
3. Review updated `.golden` files in `native/fixtures/`.
4. Commit updated fixtures alongside the code change.
5. CI will validate the new fixtures match on the next run.

---

## 6. Benchmark Baseline Protocol

Criterion stores baseline measurements in `native/target/criterion/`. These are machine-local and not committed to git. CI runs establish fresh baselines per run and detect regressions within the same run's statistical model.

For long-term tracking, benchmark results are logged in CI output and can be captured by CI analytics tools.
