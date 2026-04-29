# Substrate Benchmark Snapshot

## Scope

Epic N adds a dedicated substrate benchmark gate in `native/benches/text_substrate_bench.rs` so transcript migration closes against measured append and cursor-mapping costs rather than assumption.

Tracked suites:

- `substrate_append_*`
- `substrate_set_cursor_*`
- `substrate_byte_to_visual_*`

## Local Snapshot

Machine-local Criterion numbers vary by hardware, CPU governor, and background load. The table below is a checked-in reference snapshot from the current branch so the CI gate has a human-readable baseline to compare against.

| Suite | Meaning | Snapshot |
| --- | --- | --- |
| `substrate_append_1024` | Append one streaming token against a ~1 KiB buffer | `34.51 µs` median |
| `substrate_append_8192` | Append one streaming token against a ~8 KiB buffer | `326.38 µs` median |
| `substrate_append_65536` | Append one streaming token against a ~64 KiB buffer | `2.18 ms` median |
| `substrate_append_262144` | Append one streaming token against a ~256 KiB buffer | `7.48 ms` median |
| `substrate_set_cursor_1024` | Reconcile cursor state near the tail of a ~1 KiB wrapped buffer | `26.17 µs` median |
| `substrate_set_cursor_8192` | Reconcile cursor state near the tail of a ~8 KiB wrapped buffer | `192.97 µs` median |
| `substrate_set_cursor_65536` | Reconcile cursor state near the tail of a ~64 KiB wrapped buffer | `1.72 ms` median |
| `substrate_set_cursor_262144` | Reconcile cursor state near the tail of a ~256 KiB wrapped buffer | `5.72 ms` median |
| `substrate_byte_to_visual_1024` | Convert a near-tail byte offset in a ~1 KiB wrapped buffer | `29.24 µs` median |
| `substrate_byte_to_visual_8192` | Convert a near-tail byte offset in a ~8 KiB wrapped buffer | `228.22 µs` median |
| `substrate_byte_to_visual_65536` | Convert a near-tail byte offset in a ~64 KiB wrapped buffer | `2.26 ms` median |
| `substrate_byte_to_visual_262144` | Convert a near-tail byte offset in a ~256 KiB wrapped buffer | `8.66 ms` median |

## Reading The Results

- Append cost should not curve sharply upward with buffer size for typical transcript workloads.
- Cursor and byte-to-visual costs are expected to grow with prefix length on the current flat-string metadata path; this report exists to make that growth explicit.
- The current branch still shows the expected O(N) growth, but the measured medians stay comfortably sub-millisecond through 8 KiB and remain within a single-frame budget at 64-256 KiB block sizes. Given transcript state now shards content per block instead of forcing a single monolithic buffer, this is acceptable for Epic N closeout while still worth tracking.
