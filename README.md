# ⚠️ EXPERIMENTAL - NOT PRODUCTION READY

Built in 2 days. Expect bugs, breaking changes, incomplete implementations. Use at your own risk.

---

# Kraken TUI

TUI library: Rust (layout/rendering) + TypeScript/Bun (API).

## Status

- **Implementation**: 74 exported FFI symbols (73 public production symbols; excludes test-only `tui_init_headless`)
- **Testing**: Rust unit/integration + TypeScript FFI test suites
- **Performance**: FFI overhead ~0.085μs, 300 mutations @ 0.183ms/frame
- **API Stability**: Experimental until public v1 GA
- **Production Ready**: No

### Available Widgets

- **Box** - Flexbox container
- **Text** - Markdown + syntax highlighting
- **Input** - Text input (supports password masking)
- **Select** - Dropdown
- **ScrollBox** - Scrollable container

### Features

- Markdown rendering (pulldown-cmark)
- Syntax highlighting (syntect)
- Focus navigation (Tab/BackTab)
- Mouse events (click, scroll wheel)

## Quick Start

```bash
# Enter devenv
devenv shell

# Build native core
cargo build --release --manifest-path native/Cargo.toml

# Run tests
cargo test --manifest-path native/Cargo.toml

# Run FFI tests
cargo build --release --manifest-path native/Cargo.toml && bun test ts/test-ffi.test.ts

# Run demo
cargo build --release --manifest-path native/Cargo.toml && bun run examples/demo.ts
```

### Hello World

```typescript
import { Kraken, Box, Text } from "./ts/src/index";

const app = Kraken.init();

const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
});
const label = new Text({
	content: "Hello, Kraken!",
	fg: "#00FF00",
	bold: true,
});
root.append(label);
app.setRoot(root);

let running = true;
while (running) {
	app.readInput(16); // 16ms timeout ≈ 60fps
	for (const event of app.drainEvents()) {
		if (event.type === "key" && event.keyCode === 0x010e) running = false; // Escape
	}
	app.render();
}

app.shutdown();
```

## Limitations

- No documentation beyond source code comments
- Limited error handling - panics instead of graceful errors
- Untested edge cases remain
- No accessibility support
- Animation primitives/chaining are still being finalized for public v1 readiness
- Basic widgets only

## Contributing

Run benchmarks on your machine:

```bash
cargo build --release --manifest-path native/Cargo.toml && bun run ts/bench-ffi.ts
```

Contributions welcome. This is a learning exercise - don't expect perfect code.

## License

Apache License 2.0 - See [LICENSE.md](./LICENSE.md)
