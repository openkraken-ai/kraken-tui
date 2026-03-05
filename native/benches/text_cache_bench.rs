//! Benchmark suite for text cache (ADR-T25).
//!
//! Measures parse throughput with and without caching, and eviction pressure.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kraken_tui::text_cache;
use kraken_tui::types::{CellAttrs, ContentFormat, StyledSpan, TextCache, TextCacheKey};

fn make_spans(text: &str) -> Vec<StyledSpan> {
    vec![StyledSpan {
        text: text.to_string(),
        attrs: CellAttrs::empty(),
        fg: 0,
        bg: 0,
    }]
}

fn make_key(content_hash: u64) -> TextCacheKey {
    TextCacheKey {
        content_hash,
        format: ContentFormat::Markdown as u8,
        language_hash: 0,
        wrap_width: 80,
        style_fingerprint: 0,
    }
}

fn bench_cache_insert_and_get(c: &mut Criterion) {
    c.bench_function("cache_insert_1000", |b| {
        b.iter(|| {
            let mut cache = TextCache::new(8_388_608);
            for i in 0..1000u64 {
                let key = make_key(i);
                text_cache::insert(&mut cache, key, make_spans("benchmark content"));
            }
            black_box(&cache);
        });
    });

    c.bench_function("cache_get_hit_1000", |b| {
        let mut cache = TextCache::new(8_388_608);
        let key = make_key(42);
        text_cache::insert(&mut cache, key.clone(), make_spans("cached content"));

        b.iter(|| {
            for _ in 0..1000 {
                black_box(text_cache::get(&mut cache, &key));
            }
        });
    });
}

fn bench_eviction_pressure(c: &mut Criterion) {
    c.bench_function("cache_eviction_pressure", |b| {
        b.iter(|| {
            // Small cache that forces frequent eviction
            let mut cache = TextCache::new(5000);
            for i in 0..500u64 {
                let key = make_key(i);
                text_cache::insert(
                    &mut cache,
                    key,
                    make_spans(&format!("eviction_data_{i:0>200}")),
                );
            }
            black_box(&cache);
        });
    });
}

criterion_group!(benches, bench_cache_insert_and_get, bench_eviction_pressure);
criterion_main!(benches);
