# ADR-004: Reconciler Strategy - Imperative Core API First, Solid Later

## Status

**Accepted**

## Context

We need to decide how to map component-based UI (React/Solid) to our imperative handle-based API. This affects:

- Developer experience
- Performance characteristics
- Implementation complexity

## Decision

We will:

1. **Phase 1**: Ship with an **imperative core API** (direct handle manipulation)
2. **Phase 2**: Add **Solid.js renderer** (recommended first)
3. **Phase 3**: Consider React renderer if demand exists

## Rationale

### Why Solid First (vs. React)

1. **Fine-grained Reactivity**: Solid's signals map naturally to handle mutations:

   ```typescript
   // Solid: signal change → single FFI call
   const [count, setCount] = createSignal(0);
   createEffect(() => {
   	tui_set_content(label, String(count()), count().toString().length);
   });
   ```

   vs React's full-tree diffing which would require traversing entire widget tree.

2. **No Virtual DOM**: Solid compiles directly to DOM updates, reducing FFI overhead.

3. **Simpler Reconciler**: Solid's `createRenderer` API is simpler than react-reconciler.

4. **Memory Efficiency**: Solid's approach generates less GC pressure in JS runtime.

### Why Not React First

- React's reconciliation algorithm is designed for DOM, not handle-based APIs
- Every state change could trigger full-tree traversal across FFI boundary
- Memory overhead (50MB for simple Ink apps vs 13MB for Bubble Tea)

### Existing Precedent

- **solid-ink** (https://github.com/devinxi/solid-ink): Solid for CLI apps
- Demonstrates Solid works well for terminal UIs

## Implementation Priority

```
v1 (MVP):     Imperative API only
v2:           + Solid.js renderer
v2.x:         + React renderer (optional)
```

## API Design for Framework Integration

### Solid.js Renderer

```typescript
// Solid component maps to handle
const Counter = () => {
  const [count, setCount] = createSignal(0);
  
  return (
    <Box>
      <Text>Count: {count()}</Text>
      <Button onClick={() => setCount(c => c + 1)}>Increment</Button>
    </Box>
  );
};
```

### React Renderer (Future)

```typescript
// React component maps to handle
const Counter = () => {
  const [count, setCount] = useState(0);
  
  return (
    <Box>
      <Text>Count: {count}</Text>
      <Button onClick={() => setCount(c => c + 1)}>Increment</Button>
    </Box>
  );
};
```
