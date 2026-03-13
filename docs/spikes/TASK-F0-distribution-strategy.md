# TASK-F0 Spike: Artifact Packaging and Release Matrix Workflow (ADR-T29)

## Scope

- Define artifact naming convention for prebuilt native binaries.
- Define the minimum target matrix and Rust triple mapping.
- Define checksum and verification strategy.
- Define runtime resolution order and fallback behavior.
- Define publication target and release workflow trigger.

## Target Matrix

| Rust Target Triple           | Platform | Arch  | Library Name            |
| ---------------------------- | -------- | ----- | ----------------------- |
| x86_64-unknown-linux-gnu     | linux    | x64   | libkraken_tui.so        |
| aarch64-unknown-linux-gnu    | linux    | arm64 | libkraken_tui.so        |
| x86_64-apple-darwin          | darwin   | x64   | libkraken_tui.dylib     |
| aarch64-apple-darwin         | darwin   | arm64 | libkraken_tui.dylib     |
| x86_64-pc-windows-msvc       | win32    | x64   | kraken_tui.dll          |

Platform and arch values match `process.platform` and `process.arch` from the Bun/Node runtime.

## Artifact Naming Convention

Release artifacts follow the pattern:

```
kraken-tui-v{version}-{platform}-{arch}.{ext}
```

Examples:
- `kraken-tui-v0.1.0-linux-x64.so`
- `kraken-tui-v0.1.0-darwin-arm64.dylib`
- `kraken-tui-v0.1.0-win32-x64.dll`

Each artifact has a companion SHA-256 checksum sidecar:
- `kraken-tui-v0.1.0-linux-x64.so.sha256`

## Checksum Strategy

- **Algorithm:** SHA-256
- **Format:** `<hex-digest>  <filename>` (GNU coreutils `sha256sum` compatible)
- **Verification:** Runtime resolver reads the `.sha256` sidecar and compares against the computed hash of the loaded artifact when both files are present. Checksum mismatch raises a diagnostic error. Missing sidecar is non-fatal (supports development builds without checksums).

## Publication Target

- **Platform:** GitHub Releases
- **Trigger:** Git tag push matching `v*` pattern, plus manual `workflow_dispatch`
- **Artifact upload:** Each matrix job uploads its artifact and checksum as release assets
- **One release per version tag** — all 5 target artifacts attached to the same release

## Artifact Resolution Order

The runtime resolver (`ts/src/resolver.ts`) searches for the native library in this order:

1. **Environment override:** `KRAKEN_LIB_PATH` environment variable — absolute path to the library file. If set and the file exists, use it directly. Intended for CI, custom deployments, and debugging.

2. **Prebuilt artifacts:** `<package-root>/prebuilds/<platform>-<arch>/<libName>` — where `<package-root>` is the `ts/` directory. Prebuilds are populated by a postinstall step or manual download.

3. **Source build (dev mode):** `<package-root>/../native/target/release/<libName>` — the standard Cargo release output. This is the existing behavior for developers building from source.

4. **Failure:** If none of the above paths resolve, throw a `KrakenError` with platform-specific diagnostic information and remediation steps.

## Fallback Behavior

- The resolver does **not** auto-invoke `cargo build`. Building from source is an explicit developer action, not an implicit fallback. This prevents unexpected long compilation times and avoids requiring a Rust toolchain on end-user machines.
- Each failed search path is recorded and included in the diagnostic error message.
- The diagnostic message includes platform-specific remediation:
  - Linux: check glibc availability
  - macOS: verify architecture compatibility
  - Windows: ensure MSVC runtime
  - All: instructions for manual source build and `KRAKEN_LIB_PATH` override

## CI Workflow Design

- **Runner mapping:**
  - `ubuntu-latest` — linux-x64 (native), linux-arm64 (cross-compilation via `cross`)
  - `macos-latest` — darwin-arm64 (native), darwin-x64 (cross or universal)
  - `windows-latest` — win32-x64 (native)
- **Post-build steps:** Strip debug symbols, generate SHA-256 checksum, rename to convention, upload to release
- **Cross-compilation:** linux-arm64 uses the `cross` tool for reliable cross-compilation without qemu

## Measurable Acceptance

- All 5 target artifacts build successfully in the matrix CI workflow.
- Each artifact has a valid SHA-256 checksum sidecar.
- The runtime resolver correctly selects the matching prebuilt artifact on supported platforms.
- Unsupported platforms receive a diagnostic error with actionable remediation steps.
- The `KRAKEN_LIB_PATH` override works for all platforms.
- Existing source-build development workflow remains functional with zero behavior change.
