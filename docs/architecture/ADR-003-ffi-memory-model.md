# Phase 2: Core Architecture Decisions

## ADR-003: FFI Memory Model - Handle Allocation, Ownership, and Lifetime

### Status

**Accepted**

### Context

We need to define how memory is managed across the FFI boundary between Rust and TypeScript. This includes:
- How node handles are allocated and tracked
- Who owns each resource (Rust or TS)
- How to prevent leaks and use-after-free bugs
- How string data passes across the boundary

### Decision

We will use an **opaque handle model with Rust-owned allocations** and explicit destroy semantics:

1. **Handles**: Opaque `u32` integers that index into a Rust-side `HashMap`
2. **Ownership**: Rust owns all heap allocations; TypeScript holds handles
3. **Lifetime**: Handles remain valid until explicitly destroyed
4. **Strings**: Copy-in (TS→Rust) for input, borrowed pointer (Rust→TS) for output

### Detailed Design

#### Handle System

```rust
// Rust side
pub type NodeHandle = u32;

pub struct TuiContext {
    tree: TaffyTree<()>,
    nodes: HashMap<NodeHandle, TuiNode>,
    next_handle: NodeHandle,
}
```

- `NodeHandle` is a simple `u32` (0 = invalid/null)
- Handles are allocated sequentially by Rust
- TypeScript never sees raw pointers, only handles
- This provides a stable API even if internal storage changes

#### Ownership Model

| Resource | Owner | Destroy Method |
|----------|-------|----------------|
| Node data (TuiNode) | Rust | `tui_destroy_node(handle)` |
| Taffy tree nodes | Rust | Auto-cleaned with parent |
| Style data | Rust | Auto-cleaned with node |
| Content string | Rust | Auto-cleaned with node |
| Input strings | TS (caller) | N/A (copied into Rust) |
| Output strings | Rust | Must use `tui_free_string()` |

#### String Handling

**TS → Rust (input):**
```typescript
// TypeScript creates string, passes to Rust
const content = new CString("Hello");
lib.symbols.tui_set_content(handle, content.ptr, content.byteLength);
// Rust copies the string, TS can free immediately after call
```

**Rust → TS (output):**
```rust
// Rust allocates and returns pointer
#[no_mangle]
pub extern "C" fn tui_get_content(handle: NodeHandle) -> *const u8 {
    // Returns pointer to internally-owned string
    // TS MUST call tui_free_string() to prevent leak
}
```

#### Memory Safety Rules

1. **Handle Validity**: Always check return values. 0 or negative = error.
2. **Destroy Order**: Must destroy children before parent (or let Rust cascade).
3. **No Caching**: Don't cache handles across TuiContext lifetimes.
4. **String Ownership**: Never retain pointers returned from Rust.

### Rationale

1. **Simplicity**: Opaque handles are easier to use than raw pointers
2. **Safety**: Rust manages all allocations, no double-free possible
3. **Performance**: Handles are cheap to copy, no pointer chasing
4. **Debuggability**: Handle 0 can indicate errors consistently

### Alternative Considered

**Raw pointers (rejected)**:
- More error-prone in TypeScript
- Requires manual memory management in TS
- Harder to debug lifetime issues

**Reference counting (rejected)**:
- Complex across FFI boundary
- Adds overhead to every operation
- Circular reference risks

### Implementation Notes

1. Use `HashMap<NodeHandle, TuiNode>` for O(1) lookups
2. Recycle handles? (No - simpler to increment counter)
3. Handle overflow? (u32 max ≈ 4 billion, practical limit)
4. Thread safety? (Single-threaded TUI is acceptable for v1)

### Security Considerations

- Validate all handle inputs before dereferencing
- Handle 0 is always invalid - never allocate it
- Limit string lengths to prevent DoS
- No user-controlled pointers in Rust→TS direction
