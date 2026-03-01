## ⚠️ EXPERIMENTAL - NOT PRODUCTION READY

# Kraken TUI

Rust-native terminal UI engine with TypeScript/Bun bindings.

## Project Status

`Kraken TUI` is pre-GA and not production-ready yet.

- v1 is complete (themes + animation foundation)
- v2 is in progress
- Native state hardening, tree operations, feature expansion, and JSX reconciler are implemented
- Accessibility foundation remains the main open v2 area

For scope and planning details, see:
- [PRD](./docs/PRD.md)
- [Architecture](./docs/Architecture.md)
- [TechSpec](./docs/TechSpec.md)
- [Tasks](./docs/Tasks.md)

## Architecture

- Native core: Rust `cdylib` owns all mutable state and rendering
- Host API: TypeScript/Bun wrapper over `bun:ffi`
- Boundary invariant: TypeScript holds opaque `u32` handles, Rust owns data
- FFI contract: `0` success, `-1` error (via `tui_get_last_error()`), `-2` panic caught at boundary

## Implemented Capabilities

Widgets:
- `Box`
- `Text` (plain + markdown + syntax highlight pipeline)
- `Input` (single-line + password masking)
- `Select`
- `ScrollBox`
- `TextArea` (multi-line editing)

Core features:
- Flexbox layout via Taffy
- Incremental render/diff pipeline
- Keyboard focus traversal + mouse events (click/scroll/hit-test)
- Theming (`Theme.dark()`, `Theme.light()`, runtime switching, per-NodeType defaults)
- Animation primitives + chaining + additional easing/position animation
- Choreography groups (`tui_create_choreo_group`, add/start/cancel/destroy)
- Reconciler primitives (`destroy_subtree`, indexed insert) for declarative updates
- JSX + signal-driven reconciler (`render`, `mount`, `reconcileChildren`)

## Quick Start

```bash
# Build native core (required before TS usage)
cargo build --manifest-path native/Cargo.toml --release

# Run Rust tests
cargo test --manifest-path native/Cargo.toml

# Run TS FFI tests
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-ffi.test.ts

# Run JSX tests
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-jsx.test.ts

# Run demos
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/migration-jsx.tsx
```

## Imperative Example

```ts
import { Kraken, Box, Text, KeyCode } from "./ts/src/index";

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

## Quality and Budgets

```bash
# Bundle budget (<50KB target for core TS package)
bun run ts/check-bundle.ts

# FFI benchmark
cargo build --manifest-path native/Cargo.toml --release && bun run ts/bench-ffi.ts

# Guardrails
cargo build --manifest-path native/Cargo.toml --release && bun run ts/guardrails-ffi.ts
```

## License

Apache License 2.0 - See [LICENSE.md](./LICENSE.md)
