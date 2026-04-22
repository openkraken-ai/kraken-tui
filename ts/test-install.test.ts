/**
 * Install smoke tests — verify artifact resolver and diagnostics.
 *
 * These tests validate the cross-platform distribution UX (Epic F, ADR-T29)
 * without requiring the native library to be loaded.
 *
 * Run:  bun test ts/test-install.test.ts
 */

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { copyFileSync, existsSync, mkdirSync, rmSync } from "fs";
import { resolve } from "path";
import { resolveLibraryPath, getLibraryName } from "./src/resolver";
import { formatLoadError } from "./src/diagnostics";

// ── Library name mapping ────────────────────────────────────────────────────

describe("getLibraryName", () => {
	test("returns .dylib for darwin", () => {
		expect(getLibraryName("darwin")).toBe("libkraken_tui.dylib");
	});

	test("returns .dll for win32", () => {
		expect(getLibraryName("win32")).toBe("kraken_tui.dll");
	});

	test("returns .so for linux", () => {
		expect(getLibraryName("linux")).toBe("libkraken_tui.so");
	});

	test("returns .so for unknown platforms", () => {
		expect(getLibraryName("freebsd")).toBe("libkraken_tui.so");
	});
});

// ── Resolver ────────────────────────────────────────────────────────────────

describe("resolveLibraryPath", () => {
	const originalEnv = process.env.KRAKEN_LIB_PATH;
	const packageRoot = resolve(import.meta.dir, "src", "..");
	const stagedDir = resolve(
		packageRoot,
		"prebuilds",
		`${process.platform}-${process.arch}`,
	);
	const stagedLibPath = resolve(stagedDir, getLibraryName(process.platform));
	const sourceBuild = resolve(
		import.meta.dir,
		`../native/target/release/${getLibraryName(process.platform)}`,
	);
	let createdStagedDir = false;
	let createdStagedLib = false;

	afterEach(() => {
		if (originalEnv === undefined) {
			delete process.env.KRAKEN_LIB_PATH;
		} else {
			process.env.KRAKEN_LIB_PATH = originalEnv;
		}

		if (createdStagedLib && existsSync(stagedLibPath)) {
			rmSync(stagedLibPath, { force: true });
		}
		if (createdStagedDir && existsSync(stagedDir)) {
			rmSync(stagedDir, { recursive: true, force: true });
		}
		createdStagedDir = false;
		createdStagedLib = false;
	});

	test("resolves source build path in development", () => {
		delete process.env.KRAKEN_LIB_PATH;
		const libPath = resolveLibraryPath();
		// In this repo, the source build should exist at native/target/release/
		expect(libPath).toContain("native/target/release/");
		expect(libPath).toContain("kraken_tui");
	});

	test("respects KRAKEN_LIB_PATH env override", () => {
		// Point to the actual source build so it resolves (platform-aware)
		process.env.KRAKEN_LIB_PATH = sourceBuild;
		const libPath = resolveLibraryPath();
		expect(libPath).toBe(sourceBuild);
	});

	test("prefers staged prebuild artifact when present", () => {
		delete process.env.KRAKEN_LIB_PATH;
		const hadStagedDir = existsSync(stagedDir);
		const hadStagedLib = existsSync(stagedLibPath);
		if (!hadStagedDir) {
			mkdirSync(stagedDir, { recursive: true });
			createdStagedDir = true;
		}
		if (!hadStagedLib) {
			copyFileSync(sourceBuild, stagedLibPath);
			createdStagedLib = true;
		}

		const libPath = resolveLibraryPath();
		expect(libPath).toBe(stagedLibPath);
	});

	test("throws when KRAKEN_LIB_PATH points to nonexistent file", () => {
		process.env.KRAKEN_LIB_PATH = "/nonexistent/path/libkraken_tui.so";
		// Should still resolve via fallback (source build exists)
		const libPath = resolveLibraryPath();
		expect(libPath).toContain("native/target/release/");
	});
});

// ── Diagnostics ─────────────────────────────────────────────────────────────

describe("formatLoadError", () => {
	test("includes platform and architecture in error message", () => {
		const msg = formatLoadError("linux", "x64", ["/path/a", "/path/b"]);
		expect(msg).toContain("linux-x64");
	});

	test("includes all searched paths", () => {
		const paths = ["/first/path", "/second/path", "/third/path"];
		const msg = formatLoadError("darwin", "arm64", paths);
		for (const p of paths) {
			expect(msg).toContain(p);
		}
	});

	test("includes linux-specific remediation for linux platform", () => {
		const msg = formatLoadError("linux", "x64", []);
		expect(msg).toContain("glibc");
		expect(msg).toContain("apt install");
	});

	test("includes darwin-specific remediation for darwin platform", () => {
		const msg = formatLoadError("darwin", "arm64", []);
		expect(msg).toContain("Apple Silicon");
	});

	test("includes windows-specific remediation for win32 platform", () => {
		const msg = formatLoadError("win32", "x64", []);
		expect(msg).toContain("Visual C++");
	});

	test("always includes source build instruction", () => {
		const msg = formatLoadError("linux", "x64", []);
		expect(msg).toContain("cargo build --manifest-path native/Cargo.toml --release");
	});

	test("always includes KRAKEN_LIB_PATH override instruction", () => {
		const msg = formatLoadError("linux", "x64", []);
		expect(msg).toContain("KRAKEN_LIB_PATH");
	});
});
