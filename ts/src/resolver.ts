/**
 * Native library resolver — platform detection and artifact search.
 *
 * Resolves the path to the kraken-tui native shared library using a deterministic
 * search order: env override → prebuilt artifacts → source build → diagnostic error.
 *
 * Part of Epic F (ADR-T29).
 */

import { existsSync } from "fs";
import { resolve } from "path";
import { formatLoadError } from "./diagnostics";

/** Map process.platform to the native library filename. */
function getLibraryName(platform: string): string {
	switch (platform) {
		case "darwin":
			return "libkraken_tui.dylib";
		case "win32":
			return "kraken_tui.dll";
		default:
			return "libkraken_tui.so";
	}
}

/**
 * Resolve the native library path synchronously.
 *
 * Search order:
 * 1. KRAKEN_LIB_PATH env var (explicit override)
 * 2. Prebuilt: <packageRoot>/prebuilds/<platform>-<arch>/<libName>
 * 3. Source build: <packageRoot>/../native/target/release/<libName>
 * 4. Throw with diagnostic error
 */
export function resolveLibraryPath(): string {
	const platform = process.platform;
	const arch = process.arch;
	const libName = getLibraryName(platform);
	const searchPaths: string[] = [];

	// 1. Environment override
	const envPath = process.env.KRAKEN_LIB_PATH;
	if (envPath) {
		if (existsSync(envPath)) {
			return envPath;
		}
		searchPaths.push(`${envPath} (KRAKEN_LIB_PATH)`);
	}

	// Package root is the ts/ directory (parent of src/)
	const packageRoot = resolve(import.meta.dir, "..");

	// 2. Prebuilt artifacts
	const prebuiltPath = resolve(
		packageRoot,
		"prebuilds",
		`${platform}-${arch}`,
		libName,
	);
	searchPaths.push(prebuiltPath);
	if (existsSync(prebuiltPath)) {
		return prebuiltPath;
	}

	// 3. Source build (development mode)
	const sourceBuildPath = resolve(
		packageRoot,
		"..",
		"native",
		"target",
		"release",
		libName,
	);
	searchPaths.push(sourceBuildPath);
	if (existsSync(sourceBuildPath)) {
		return sourceBuildPath;
	}

	// 4. Failure with diagnostics
	throw new Error(formatLoadError(platform, arch, searchPaths));
}

// Re-export for testing
export { getLibraryName };
