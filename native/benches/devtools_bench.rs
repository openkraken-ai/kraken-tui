//! Devtools overhead benchmark (TechSpec §5.5)
//!
//! Measures the overhead of debug mode operations:
//! - push_trace no-op path (debug_mode=false must be negligible)
//! - push_trace active path (debug_mode=true, ring bounded at 256)
//! - build_snapshot_json serialization cost
//! - take_frame_snapshot cost
//!
//! Per TechSpec §5.5, debug-off overhead must stay below 3% render delta.
//! The debug-off benchmarks directly measure this short-circuit overhead.
//!
//! Run with: cargo bench --manifest-path native/Cargo.toml --bench devtools_bench

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use kraken_tui::devtools::bench_workloads::{
    make_context, run_build_snapshot, run_push_traces, run_take_snapshot,
};

fn bench_push_trace_debug_off(c: &mut Criterion) {
    // When debug_mode=false, push_trace must short-circuit with negligible cost.
    c.bench_function("push_trace_debug_off_100", |b| {
        b.iter_batched(
            || make_context(false),
            |mut ctx| {
                run_push_traces(&mut ctx, black_box(100));
                black_box(ctx)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_push_trace_debug_on(c: &mut Criterion) {
    // Active path — fills ring up to 256 cap.
    c.bench_function("push_trace_debug_on_100", |b| {
        b.iter_batched(
            || make_context(true),
            |mut ctx| {
                run_push_traces(&mut ctx, black_box(100));
                black_box(ctx)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_snapshot_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_json");

    for n_traces in [0usize, 50, 256].iter() {
        group.bench_with_input(
            BenchmarkId::new("build_snapshot", n_traces),
            n_traces,
            |b, &n| {
                b.iter_batched(
                    || {
                        let mut ctx = make_context(true);
                        run_push_traces(&mut ctx, n);
                        run_take_snapshot(&mut ctx);
                        ctx
                    },
                    |ctx| {
                        let json = run_build_snapshot(black_box(&ctx));
                        black_box(json)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_take_snapshot(c: &mut Criterion) {
    c.bench_function("take_frame_snapshot", |b| {
        b.iter_batched(
            || make_context(true),
            |mut ctx| {
                run_take_snapshot(&mut ctx);
                black_box(ctx)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_push_trace_debug_off,
    bench_push_trace_debug_on,
    bench_snapshot_json,
    bench_take_snapshot
);
criterion_main!(benches);
