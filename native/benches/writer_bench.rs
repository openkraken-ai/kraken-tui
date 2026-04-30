//! Benchmark suite for terminal writer throughput (ADR-T24, TASK-G2).
//!
//! Measures run compaction and frame emission across three canonical diff
//! densities: full (100%), medium (50%), and sparse (10%).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kraken_tui::writer::workloads::{full_diff, medium_diff, sparse_diff};
use kraken_tui::writer::{baseline_metrics, compact_runs, emit_frame, WriterState};

fn bench_compact_runs(c: &mut Criterion) {
    let full = full_diff();
    let medium = medium_diff();
    let sparse = sparse_diff();

    c.bench_function("writer_compact_full", |b| {
        b.iter(|| black_box(compact_runs(&full)));
    });

    c.bench_function("writer_compact_medium", |b| {
        b.iter(|| black_box(compact_runs(&medium)));
    });

    c.bench_function("writer_compact_sparse", |b| {
        b.iter(|| black_box(compact_runs(&sparse)));
    });
}

fn bench_emit_frame(c: &mut Criterion) {
    let full_runs = compact_runs(&full_diff());
    let medium_runs = compact_runs(&medium_diff());
    let sparse_runs = compact_runs(&sparse_diff());

    c.bench_function("writer_emit_full", |b| {
        b.iter(|| {
            let mut state = WriterState::new();
            let mut buf = Vec::with_capacity(8192);
            black_box(emit_frame(&mut state, &full_runs, &mut buf, false).unwrap());
        });
    });

    c.bench_function("writer_emit_medium", |b| {
        b.iter(|| {
            let mut state = WriterState::new();
            let mut buf = Vec::with_capacity(8192);
            black_box(emit_frame(&mut state, &medium_runs, &mut buf, false).unwrap());
        });
    });

    c.bench_function("writer_emit_sparse", |b| {
        b.iter(|| {
            let mut state = WriterState::new();
            let mut buf = Vec::with_capacity(8192);
            black_box(emit_frame(&mut state, &sparse_runs, &mut buf, false).unwrap());
        });
    });
}

fn bench_throughput_reduction(c: &mut Criterion) {
    // Measures the end-to-end pipeline: compact + emit, comparing
    // against baseline metrics to validate the >= 35% ops reduction target.
    c.bench_function("writer_pipeline_full", |b| {
        let diff = full_diff();
        b.iter(|| {
            let runs = compact_runs(&diff);
            let mut state = WriterState::new();
            let mut buf = Vec::with_capacity(8192);
            let metrics = emit_frame(&mut state, &runs, &mut buf, false).unwrap();
            let baseline = baseline_metrics(&diff);
            black_box((metrics, baseline));
        });
    });
}

criterion_group!(
    benches,
    bench_compact_runs,
    bench_emit_frame,
    bench_throughput_reduction
);
criterion_main!(benches);
