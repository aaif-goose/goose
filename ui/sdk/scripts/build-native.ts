#!/usr/bin/env node
/**
 * Builds the goose binary for target platforms and places them
 * into the corresponding npm package directories under ui/goose-binary/.
 *
 * Usage:
 *   npm run build:native              # build for current platform only
 *   npm run build:native:all          # build for all platforms
 *   tsx scripts/build-native.ts darwin-arm64  # build specific platform
 *
 * Prerequisites:
 *   - Rust cross-compilation toolchains installed for each target
 */

import { execSync } from "child_process";
import { dirname, resolve } from "path";
import { fileURLToPath } from "url";
import { mkdirSync, copyFileSync, chmodSync, existsSync } from "fs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = resolve(__dirname, "../../..");
const NATIVE_DIR = resolve(ROOT, "ui/goose-binary");

const RUST_TARGETS: Record<string, string> = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "win32-x64": "x86_64-pc-windows-msvc",
};

const PLATFORM_MAP: Record<string, string> = {
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "linux-arm64": "linux-arm64",
  "linux-x64": "linux-x64",
  "win32-x64": "win32-x64",
};

function getCurrentPlatform(): string | null {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;
  return PLATFORM_MAP[key] || null;
}

function buildTarget(platform: string): void {
  const rustTarget = RUST_TARGETS[platform];
  if (!rustTarget) {
    throw new Error(`Unknown platform: ${platform}`);
  }

  const pkgDir = resolve(NATIVE_DIR, `goose-binary-${platform}`);
  const binDir = resolve(pkgDir, "bin");

  console.log(`==> Building goose for ${platform} (${rustTarget})`);

  try {
    const featureArgs = platform.startsWith("linux-")
      ? " --features vulkan"
      : "";
    execSync(
      `cargo build --release --target ${rustTarget} --bin goose${featureArgs}`,
      {
        cwd: ROOT,
        stdio: "inherit",
      },
    );
  } catch (err) {
    console.error(`Failed to build for ${platform}`);
    throw err;
  }

  mkdirSync(binDir, { recursive: true });

  const ext = platform.startsWith("win32") ? ".exe" : "";
  const binaryName = `goose${ext}`;
  const srcPath = resolve(ROOT, "target", rustTarget, "release", binaryName);
  const destPath = resolve(binDir, binaryName);

  if (!existsSync(srcPath)) {
    throw new Error(`Binary not found at ${srcPath}`);
  }

  copyFileSync(srcPath, destPath);
  chmodSync(destPath, 0o755);

  console.log(`    ✅ Placed binary at ${destPath}`);
}

async function main() {
  const args = process.argv.slice(2);
  const buildAll = args.includes("--all");

  if (buildAll) {
    console.log("==> Building for all platforms");
    for (const platform of Object.keys(RUST_TARGETS)) {
      try {
        buildTarget(platform);
      } catch (err) {
        console.error(`Failed to build ${platform}:`, err);
        process.exit(1);
      }
    }
  } else if (args.length > 0 && !args[0].startsWith("--")) {
    // Build specific platforms
    for (const platform of args) {
      if (!RUST_TARGETS[platform]) {
        console.error(`Unknown platform: ${platform}`);
        console.error(
          `Valid platforms: ${Object.keys(RUST_TARGETS).join(", ")}`,
        );
        process.exit(1);
      }
      buildTarget(platform);
    }
  } else {
    // Build for current platform only
    const currentPlatform = getCurrentPlatform();
    if (!currentPlatform) {
      console.error(
        `Unsupported platform: ${process.platform}-${process.arch}`,
      );
      console.error(`Valid platforms: ${Object.keys(RUST_TARGETS).join(", ")}`);
      console.error(`Use --all to build for all platforms`);
      process.exit(1);
    }
    console.log(`==> Building for current platform: ${currentPlatform}`);
    buildTarget(currentPlatform);
  }

  console.log("==> Done. Native packages staged in ui/goose-binary/");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
