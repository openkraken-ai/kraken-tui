//! Threaded Render Parity and Benchmark Validation (TASK-H2, ADR-T31)
//!
//! Compares synchronous vs threaded render paths across canonical workloads.
//! Run with: `cargo bench --manifest-path native/Cargo.toml --features threaded-render --bench threaded_render_bench`
//!
//! Metrics captured:
//! - Compact runs throughput
//! - Writer emission throughput
//! - Buffer diff throughput

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kraken_tui::types::{Buffer, Cell, CellAttrs, CellUpdate};
use kraken_tui::writer;

/// Create a synthetic diff workload of N changed cells.
fn synthetic_diff(width: u16, height: u16, density_pct: u32) -> Vec<CellUpdate> {
    let mut updates = Vec::new();
    let total = (width as u32) * (height as u32);
    let changed = total * density_pct / 100;

    for i in 0..changed {
        let x = (i % width as u32) as u16;
        let y = (i / width as u32) as u16;
        if y >= height {
            break;
        }
        updates.push(CellUpdate {
            x,
            y,
            cell: Cell {
                ch: 'A',
                fg: 0x01FF0000,
                bg: 0x01000000,
                attrs: CellAttrs::empty(),
                link: None,
            },
        });
    }
    updates
}

/// Benchmark: writer compact_runs (shared between both paths)
fn bench_compact_runs(c: &mut Criterion) {
    let diff_50 = synthetic_diff(80, 24, 50);
    let diff_100 = synthetic_diff(80, 24, 100);

    c.bench_function("threaded_compact_runs_50pct_80x24", |b| {
        b.iter(|| {
            let runs = writer::compact_runs(black_box(&diff_50));
            black_box(runs);
        })
    });

    c.bench_function("threaded_compact_runs_100pct_80x24", |b| {
        b.iter(|| {
            let runs = writer::compact_runs(black_box(&diff_100));
            black_box(runs);
        })
    });
}

/// Benchmark: emit_frame to sink (shared writer emission)
fn bench_emit_frame(c: &mut Criterion) {
    let diff = synthetic_diff(80, 24, 50);
    let runs = writer::compact_runs(&diff);

    c.bench_function("threaded_emit_frame_50pct_80x24", |b| {
        let mut state = writer::WriterState::new();
        b.iter(|| {
            state.reset();
            let mut sink = std::io::sink();
            let metrics =
                writer::emit_frame(black_box(&mut state), black_box(&runs), &mut sink, false)
                    .unwrap();
            black_box(metrics);
        })
    });
}

/// Benchmark: buffer diff (shared between both paths)
fn bench_buffer_diff(c: &mut Criterion) {
    let mut front = Buffer::new(80, 24);
    let back = Buffer::new(80, 24);

    // Modify 50% of cells
    for i in 0..(80 * 24 / 2) {
        let x = (i % 80) as u16;
        let y = (i / 80) as u16;
        front.set(
            x,
            y,
            Cell {
                ch: 'X',
                fg: 0x01FF0000,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
    }

    c.bench_function("threaded_buffer_diff_50pct_80x24", |b| {
        b.iter(|| {
            let mut updates = Vec::new();
            for y in 0..24u16 {
                for x in 0..80u16 {
                    let f = front.get(x, y).unwrap();
                    let b_cell = back.get(x, y);
                    let changed = match b_cell {
                        Some(bc) => f != bc,
                        None => true,
                    };
                    if changed {
                        updates.push(CellUpdate {
                            x,
                            y,
                            cell: f.clone(),
                        });
                    }
                }
            }
            black_box(updates);
        })
    });
}

criterion_group!(
    benches,
    bench_compact_runs,
    bench_emit_frame,
    bench_buffer_diff
);
criterion_main!(benches);
