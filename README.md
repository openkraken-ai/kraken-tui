# Kraken TUI

Rust-native terminal UI engine with TypeScript/Bun bindings for building fast, long-lived terminal applications.

Kraken is aimed at developer tools, agent consoles, log viewers, and dense pane-based terminal workflows where stable scrolling, low overhead, and strong inspectability matter more than a browser-style framework stack.

## Status

- Pre-1.0 (`0.1.0`) and still evolving
- Canonical planning chain lives in [`docs/`](./docs/)
- Current flagship surfaces are already implemented in source: transcript workflows, split panes, devtools, and app-shaped composites

## Core Model

- **Native Core:** Rust `cdylib` owns all mutable UI state and performance-critical work
- **Host Layer:** TypeScript/Bun wrapper over `bun:ffi`
- **Boundary invariant:** TypeScript holds opaque `u32` Handles, Rust owns the data
- **FFI contract:** `0` success, `-1` explicit error via `tui_get_last_error()`, `-2` panic caught at the boundary

## What Kraken Ships

### Native widgets
- `Box`
- `Text`
- `Input`
- `Select`
- `ScrollBox`
- `TextArea`
- `Table`
- `List`
- `Tabs`
- `Overlay`
- `TranscriptView`
- `SplitPane`

### Host composites
- `CommandPalette`
- `TracePanel`
- `StructuredLogView`
- `CodeView`
- `DiffView`

### Platform and DX features
- Flexbox layout via Taffy
- Incremental double-buffered render with dirty diffing
- Keyboard focus traversal plus mouse hit-testing and scroll routing
- Rich text: Markdown and syntax highlighting
- Theming with built-in dark/light themes and per-NodeType defaults
- Animation with easing, chaining, choreography, and position offsets
- JSX plus `@preact/signals-core` reconciler
- Runner API with `app.run()` / `createLoop()`
- Accessibility foundation: roles, labels, descriptions, accessibility events
- Devtools: overlays, snapshots, traces, perf HUD helpers, and dev sessions
- Native artifact resolver: `KRAKEN_LIB_PATH` -> staged prebuilds -> local Cargo build

## Quick Start (Source Checkout)

```bash
# Build native core
cargo build --manifest-path native/Cargo.toml --release

# Install host dependencies
cd ts && bun install && cd ..

# Run the full host test surface
bun test ts/test-ffi.test.ts
bun test ts/test-jsx.test.ts
bun test ts/test-examples.test.ts
bun test ts/test-install.test.ts
bun test ts/test-runner.test.ts

# Run native tests
cargo test --manifest-path native/Cargo.toml
```

Repo-side FFI tests and benchmark harnesses intentionally target the local Cargo-built native artifact under `native/target/release/` so they validate the branch under review rather than any staged prebuild.

## Flagship Examples

```bash
cargo build --manifest-path native/Cargo.toml --release && bun run examples/agent-console.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/ops-log-console.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/repo-inspector.ts
```

Other examples:

```bash
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/migration-jsx.tsx
cargo build --manifest-path native/Cargo.toml --release && bun run examples/system-monitor.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/accessibility-demo.tsx
```

## Example: Imperative API

```ts
import { Kraken, Box, Text, KeyCode } from "kraken-tui";

const app = Kraken.init();

const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
});

const title = new Text({
	content: "Hello, Kraken!",
	fg: "#00FF88",
	bold: true,
	height: 1,
});

root.append(title);
app.setRoot(root);

let running = true;
while (running) {
	app.readInput(16);
	for (const event of app.drainEvents()) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) {
			running = false;
		}
	}
	app.render();
}

app.shutdown();
```

## Example: JSX + Signals

```tsx
import { Kraken, signal, render, createLoop, KeyCode } from "kraken-tui";
import { jsx, jsxs } from "kraken-tui/jsx-runtime";

const count = signal(0);
const app = Kraken.init();

const tree = jsxs("Box", {
	width: "100%",
	height: "100%",
	flexDirection: "column",
	children: [
		jsx("Text", {
			key: "label",
			content: count,
			fg: "#00FF88",
			height: 1,
		}),
	],
});

render(tree, app);

const loop = createLoop({
	app,
	onEvent(event) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) {
			loop.stop();
		}
	},
	onTick() {
		count.value++;
	},
});

await loop.start();
app.shutdown();
```

## Verification and Budgets

```bash
# Bundle budget
bun run ts/check-bundle.ts

# FFI and render benchmarks
bun run ts/bench-ffi.ts
bun run ts/bench-render.ts

# Native quality
cargo fmt --manifest-path native/Cargo.toml -- --check
cargo clippy --manifest-path native/Cargo.toml -- -D warnings
```

The host package is currently under the 75KB bundle budget; use `bun run ts/check-bundle.ts` for the exact measurement in the current checkout.

## Documentation

- [PRD](./docs/PRD.md) — product intent, glossary, scope, constraints
- [Architecture](./docs/Architecture.md) — logical boundaries, flows, risks
- [TechSpec](./docs/TechSpec.md) — ABI, state model, interfaces, verification contract
- [Tasks](./docs/Tasks.md) — active plan plus archived completed execution scope
- [GatePolicy](./docs/reports/GatePolicy.md) — current CI quality gates

## License

Apache License 2.0 — see [LICENSE.md](./LICENSE.md)
