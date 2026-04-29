//! Benchmark suite for the substrate text path (Epic N / CORE-N5).
//!
//! Measures:
//! - append cost as a function of existing buffer size
//! - cursor / byte-to-visual prefix mapping cost as a function of offset length

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

const APPEND_PAYLOAD: &str = "token-stream delta\n";
const BUFFER_SIZES: [usize; 4] = [1_024, 8_192, 65_536, 262_144];
const WRAP_WIDTH: u32 = 80;

struct BenchSession;

impl BenchSession {
    fn new() -> Self {
        assert_eq!(kraken_tui::tui_init_headless(160, 60), 0);
        Self
    }
}

impl Drop for BenchSession {
    fn drop(&mut self) {
        let _ = kraken_tui::tui_shutdown();
    }
}

fn make_seed_content(target_len: usize) -> String {
    const SEED_LINE: &str = "0123456789 abcdefghijklmnopqrstuvwxyz rust substrate bench\n";

    let mut out = String::with_capacity(target_len);
    while out.len() < target_len {
        out.push_str(SEED_LINE);
    }
    out.truncate(target_len);
    out
}

fn create_buffer_with_bytes(target_len: usize) -> u32 {
    let handle = kraken_tui::tui_text_buffer_create();
    assert!(handle > 0);
    let content = make_seed_content(target_len);
    assert_eq!(
        kraken_tui::tui_text_buffer_append(handle, content.as_ptr(), content.len() as u32),
        0
    );
    handle
}

fn destroy_buffer(handle: u32) {
    assert_eq!(kraken_tui::tui_text_buffer_destroy(handle), 0);
}

fn create_view_for_prefix_cost(target_len: usize) -> (u32, u32, u32) {
    let buffer = create_buffer_with_bytes(target_len);
    let view = kraken_tui::tui_text_view_create(buffer);
    assert!(view > 0);
    assert_eq!(
        kraken_tui::tui_text_view_set_wrap(view, WRAP_WIDTH, 1, 4),
        0
    );
    let offset = target_len.saturating_sub(APPEND_PAYLOAD.len()) as u32;
    (buffer, view, offset)
}

fn destroy_view_and_buffer(buffer: u32, view: u32) {
    assert_eq!(kraken_tui::tui_text_view_destroy(view), 0);
    destroy_buffer(buffer);
}

fn bench_append_cost(c: &mut Criterion) {
    let _session = BenchSession::new();

    for size in BUFFER_SIZES {
        let name = format!("substrate_append_{size}");
        c.bench_function(&name, |b| {
            b.iter_batched(
                || create_buffer_with_bytes(size),
                |buffer| {
                    assert_eq!(
                        kraken_tui::tui_text_buffer_append(
                            buffer,
                            black_box(APPEND_PAYLOAD.as_ptr()),
                            APPEND_PAYLOAD.len() as u32,
                        ),
                        0
                    );
                    black_box(kraken_tui::tui_text_buffer_get_byte_len(buffer));
                    destroy_buffer(buffer);
                },
                BatchSize::SmallInput,
            );
        });
    }
}

fn bench_set_cursor_prefix_cost(c: &mut Criterion) {
    let _session = BenchSession::new();

    for size in BUFFER_SIZES {
        let (buffer, view, offset) = create_view_for_prefix_cost(size);
        let name = format!("substrate_set_cursor_{size}");
        c.bench_function(&name, |b| {
            b.iter(|| {
                assert_eq!(
                    kraken_tui::tui_text_view_set_cursor(view, black_box(offset)),
                    0
                );
            });
        });
        destroy_view_and_buffer(buffer, view);
    }
}

fn bench_byte_to_visual_prefix_cost(c: &mut Criterion) {
    let _session = BenchSession::new();

    for size in BUFFER_SIZES {
        let (buffer, view, offset) = create_view_for_prefix_cost(size);
        let name = format!("substrate_byte_to_visual_{size}");
        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut row = 0u32;
                let mut col = 0u32;
                assert_eq!(
                    kraken_tui::tui_text_view_byte_to_visual(
                        view,
                        black_box(offset),
                        &mut row,
                        &mut col,
                    ),
                    0
                );
                black_box((row, col));
            });
        });
        destroy_view_and_buffer(buffer, view);
    }
}

criterion_group!(
    benches,
    bench_append_cost,
    bench_set_cursor_prefix_cost,
    bench_byte_to_visual_prefix_cost
);
criterion_main!(benches);
