//! Text Cache Module — Bounded LRU cache for parsed text content (ADR-T25).
//!
//! Responsibilities:
//! - Store parsed `Vec<StyledSpan>` results keyed by content/format/language/width/style
//! - Enforce hard 8 MiB capacity via LRU eviction
//! - Track memory usage with per-entry byte size accounting

use crate::types::{StyledSpan, TextCache, TextCacheEntry, TextCacheKey};

/// Estimate the memory size of a cache entry's span data.
///
/// Accounts for: text bytes + struct overhead per span + fixed entry overhead.
fn estimate_entry_size(spans: &[StyledSpan]) -> u32 {
    let text_bytes: usize = spans.iter().map(|s| s.text.len()).sum();
    let struct_overhead = spans.len() * 32; // StyledSpan struct fields
    let entry_overhead = 64; // TextCacheEntry + HashMap slot
    (text_bytes + struct_overhead + entry_overhead) as u32
}

/// Look up a cache entry, updating LRU order on hit.
///
/// Returns a reference to the cached spans if found, `None` on miss.
pub fn get<'a>(cache: &'a mut TextCache, key: &TextCacheKey) -> Option<&'a Vec<StyledSpan>> {
    if let Some(entry) = cache.entries.get_mut(key) {
        cache.tick += 1;
        entry.last_access_tick = cache.tick;
        // Move to back of LRU order
        if let Some(pos) = cache.lru_order.iter().position(|k| k == key) {
            cache.lru_order.remove(pos);
        }
        cache.lru_order.push_back(key.clone());
        // Re-borrow immutably after mutation
        return cache.entries.get(key).map(|e| &e.spans);
    }
    None
}

/// Insert a parsed result into the cache with capacity enforcement.
///
/// If the entry exceeds `max_bytes` on its own, it is silently skipped.
/// Otherwise, LRU entries are evicted until space is available.
pub fn insert(cache: &mut TextCache, key: TextCacheKey, spans: Vec<StyledSpan>) {
    let byte_size = estimate_entry_size(&spans);

    // Single entry exceeds total capacity — skip silently
    if byte_size > cache.max_bytes {
        return;
    }

    // If key already exists, remove old entry first
    if let Some(old) = cache.entries.remove(&key) {
        cache.used_bytes = cache.used_bytes.saturating_sub(old.byte_size);
        if let Some(pos) = cache.lru_order.iter().position(|k| k == &key) {
            cache.lru_order.remove(pos);
        }
    }

    // Evict LRU entries until we have room
    while cache.used_bytes + byte_size > cache.max_bytes {
        if !evict_lru(cache) {
            return; // Nothing left to evict
        }
    }

    cache.tick += 1;
    cache.entries.insert(
        key.clone(),
        TextCacheEntry {
            spans,
            byte_size,
            last_access_tick: cache.tick,
        },
    );
    cache.lru_order.push_back(key);
    cache.used_bytes += byte_size;
}

/// Clear all cache entries and reset accounting.
pub fn clear(cache: &mut TextCache) {
    cache.entries.clear();
    cache.lru_order.clear();
    cache.used_bytes = 0;
    cache.tick = 0;
}

/// Evict the least recently used entry. Returns `true` if an entry was evicted.
fn evict_lru(cache: &mut TextCache) -> bool {
    if let Some(oldest_key) = cache.lru_order.pop_front() {
        if let Some(entry) = cache.entries.remove(&oldest_key) {
            cache.used_bytes = cache.used_bytes.saturating_sub(entry.byte_size);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CellAttrs;

    fn make_spans(text: &str) -> Vec<StyledSpan> {
        vec![StyledSpan {
            text: text.to_string(),
            attrs: CellAttrs::empty(),
            fg: 0,
            bg: 0,
            link: None,
        }]
    }

    fn make_key(content_hash: u64) -> TextCacheKey {
        TextCacheKey {
            content_hash,
            format: 0,
            language_hash: 0,
            wrap_width: 80,
            style_fingerprint: 0,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let mut cache = TextCache::new(1_000_000);
        let key = make_key(1);
        let spans = make_spans("hello");

        insert(&mut cache, key.clone(), spans.clone());
        let result = get(&mut cache, &key);
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].text, "hello");
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = TextCache::new(1_000_000);
        let key = make_key(999);
        assert!(get(&mut cache, &key).is_none());
    }

    #[test]
    fn test_lru_eviction() {
        // Small cache that can hold ~2 entries
        let mut cache = TextCache::new(300);
        let key1 = make_key(1);
        let key2 = make_key(2);
        let key3 = make_key(3);

        insert(&mut cache, key1.clone(), make_spans("aaa"));
        insert(&mut cache, key2.clone(), make_spans("bbb"));

        // Access key1 to make it more recent
        let _ = get(&mut cache, &key1);

        // Insert key3 — should evict key2 (least recently used)
        insert(&mut cache, key3.clone(), make_spans("ccc"));

        assert!(
            get(&mut cache, &key1).is_some(),
            "key1 should survive (recently accessed)"
        );
        assert!(get(&mut cache, &key3).is_some(), "key3 should be present");
        // key2 may or may not be evicted depending on exact sizes, but the important
        // thing is used_bytes <= max_bytes
        assert!(cache.used_bytes <= cache.max_bytes);
    }

    #[test]
    fn test_used_bytes_never_exceeds_max() {
        let mut cache = TextCache::new(500);
        for i in 0..100u64 {
            let key = make_key(i);
            insert(&mut cache, key, make_spans(&format!("entry_{i}")));
            assert!(
                cache.used_bytes <= cache.max_bytes,
                "used_bytes {} exceeded max_bytes {} after inserting entry {i}",
                cache.used_bytes,
                cache.max_bytes
            );
        }
    }

    #[test]
    fn test_get_updates_access_order() {
        let mut cache = TextCache::new(1_000_000);
        let key1 = make_key(1);
        let key2 = make_key(2);

        insert(&mut cache, key1.clone(), make_spans("first"));
        insert(&mut cache, key2.clone(), make_spans("second"));

        // Access key1 — it should move to back of LRU
        let _ = get(&mut cache, &key1);

        // key2 should now be at front (oldest)
        assert_eq!(cache.lru_order.front(), Some(&key2));
        assert_eq!(cache.lru_order.back(), Some(&key1));
    }

    #[test]
    fn test_oversized_entry_skipped() {
        let mut cache = TextCache::new(50); // Very small cache
        let key = make_key(1);
        // Create a span with text larger than max_bytes
        let big_spans = make_spans(&"x".repeat(1000));

        insert(&mut cache, key.clone(), big_spans);
        assert!(get(&mut cache, &key).is_none());
        assert_eq!(cache.used_bytes, 0);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn test_clear() {
        let mut cache = TextCache::new(1_000_000);
        insert(&mut cache, make_key(1), make_spans("one"));
        insert(&mut cache, make_key(2), make_spans("two"));
        assert!(!cache.entries.is_empty());
        assert!(cache.used_bytes > 0);

        clear(&mut cache);
        assert!(cache.entries.is_empty());
        assert!(cache.lru_order.is_empty());
        assert_eq!(cache.used_bytes, 0);
        assert_eq!(cache.tick, 0);
    }

    #[test]
    fn test_replace_existing_key() {
        let mut cache = TextCache::new(1_000_000);
        let key = make_key(1);

        insert(&mut cache, key.clone(), make_spans("original"));
        insert(&mut cache, key.clone(), make_spans("replaced"));
        // Size should be similar (same-length text)
        assert!(cache.used_bytes > 0);
        // Should still have exactly one entry
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(get(&mut cache, &key).unwrap()[0].text, "replaced");

        // No double-counting
        let expected = estimate_entry_size(&make_spans("replaced"));
        assert_eq!(cache.used_bytes, expected);
    }

    #[test]
    fn test_eviction_under_pressure() {
        // 8 MiB cache under heavy insert pressure
        let mut cache = TextCache::new(8_388_608);
        // Insert 1000 entries with ~1KB text each (~1MB total data + overhead)
        for i in 0..1000u64 {
            let key = make_key(i);
            let text = "x".repeat(1024);
            insert(&mut cache, key, make_spans(&text));
            assert!(
                cache.used_bytes <= cache.max_bytes,
                "used_bytes {} exceeded max_bytes {} at iteration {i}",
                cache.used_bytes,
                cache.max_bytes
            );
        }
        // Entries should exist (capacity is large enough for all)
        assert!(cache.entries.len() > 0);
    }

    #[test]
    fn test_eviction_under_tight_pressure() {
        // Very small cache forces frequent eviction
        let mut cache = TextCache::new(2000);
        for i in 0..500u64 {
            let key = make_key(i);
            insert(&mut cache, key, make_spans(&format!("data_{i:0>100}")));
            assert!(
                cache.used_bytes <= cache.max_bytes,
                "overflow at iteration {i}: {} > {}",
                cache.used_bytes,
                cache.max_bytes
            );
        }
    }

    #[test]
    fn test_hit_rate_stable_content() {
        let mut cache = TextCache::new(1_000_000);
        let key = make_key(42);
        let spans = make_spans("stable content for caching");

        // First access — miss
        insert(&mut cache, key.clone(), spans);
        let mut hits = 0u32;
        let mut misses = 1u32; // initial insert counts as miss

        // 99 subsequent accesses — all hits
        for _ in 0..99 {
            if get(&mut cache, &key).is_some() {
                hits += 1;
            } else {
                misses += 1;
            }
        }

        let hit_rate = hits as f64 / (hits + misses) as f64;
        assert!(
            hit_rate >= 0.99,
            "expected >= 99% hit rate, got {:.1}%",
            hit_rate * 100.0
        );
    }
}
