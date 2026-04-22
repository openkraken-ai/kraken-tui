# Kraken TUI

Rust-native terminal UI engine with TypeScript/Bun bindings.

## Architecture

- **Native core:** Rust `cdylib` owns all mutable state and rendering
- **Host API:** TypeScript/Bun wrapper over `bun:ffi`
- **Boundary invariant:** TypeScript holds opaque `u32` handles, Rust owns data
- **FFI contract:** `0` success, `-1` error (via `tui_get_last_error()`), `-2` panic caught at boundary

## Widgets

- `Box` — container with flexbox layout
- `Text` — plain, markdown, and syntax-highlighted content
- `Input` — single-line text entry with password masking
- `Select` — option list with arrow navigation
- `ScrollBox` — scrollable container
- `TextArea` — multi-line editor with selection, undo/redo, find
- `Table` — columnar data with row selection
- `List` — item list with selection
- `Tabs` — tab labels with active index
- `Overlay` — modal dialog with dismiss-on-escape

## Features

- Flexbox layout via Taffy
- Incremental double-buffered render with dirty-region diffing
- Keyboard focus traversal (depth-first) + mouse events (click/scroll/hit-test)
- Theming: built-in dark/light, custom themes, per-NodeType defaults, runtime switching
- Animation: property transitions, 8 easing functions, chaining, choreography groups, position animation
- JSX + signal-driven reconciler (`@preact/signals-core`)
- Runner API with `app.run()` and `createLoop()` for async event loops
- Accessibility foundation: roles, labels, descriptions, a11y events
- Rich text: Markdown (pulldown-cmark) and syntax highlighting (syntect)
- Terminal writer with run compaction and style delta tracking
- Bounded LRU text cache (8 MiB)
- Cross-platform: Linux x64/arm64, macOS x64/arm64, Windows x64

## Quick Start

```bash
# Build native core (required before any TS usage)
cargo build --manifest-path native/Cargo.toml --release

# Install TS dependencies
cd ts && bun install && cd ..

# Run Rust tests
cargo test --manifest-path native/Cargo.toml

# Run TS tests
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-ffi.test.ts
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-jsx.test.ts

# Run examples
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/migration-jsx.tsx
cargo build --manifest-path native/Cargo.toml --release && bun run examples/system-monitor.ts
```

## Imperative Example

```ts
import { Kraken, Box, Text, KeyCode } from "kraken-tui";

const app = Kraken.init();
const root = new Box({ width: "100%", height: "100%", flexDirection: "column" });
const label = new Text({ content: "Hello, Kraken!", fg: "#00FF00", bold: true });
root.append(label);
app.setRoot(root);

let running = true;
while (running) {
	app.readInput(16);
	for (const event of app.drainEvents()) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) running = false;
	}
	app.render();
}

app.shutdown();
```

## JSX Example

```tsx
import { Kraken, signal, render, createLoop, KeyCode } from "kraken-tui";
import { jsx, jsxs } from "kraken-tui/jsx-runtime";

const count = signal(0);
const app = Kraken.init();

const tree = jsxs("Box", {
	width: "100%", height: "100%", flexDirection: "column",
	children: [
		jsx("Text", { key: "label", content: count, fg: "#00FF00", height: 1 }),
	],
});

render(tree, app);
const loop = createLoop({
	app,
	onEvent(e) {
		if (e.type === "key" && e.keyCode === KeyCode.Escape) loop.stop();
	},
	onTick() { count.value++; },
});
await loop.start();
app.shutdown();
```

## Quality and Budgets

```bash
# Bundle budget (<75KB target for core TS package)
bun run ts/check-bundle.ts

# FFI benchmark
cargo build --manifest-path native/Cargo.toml --release && bun run ts/bench-ffi.ts

# Lint
cargo fmt --manifest-path native/Cargo.toml && cargo clippy --manifest-path native/Cargo.toml
```

## Documentation

- [PRD](./docs/PRD.md) — Product requirements
- [Architecture](./docs/Architecture.md) — System design and module boundaries
- [TechSpec](./docs/TechSpec.md) — Technical contracts, FFI surface, ADRs
- [Tasks](./docs/Tasks.md) — Execution status

## License

Apache License 2.0 - See [LICENSE.md](./LICENSE.md)
