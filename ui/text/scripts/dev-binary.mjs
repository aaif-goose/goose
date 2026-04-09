#!/usr/bin/env node

// For development: ensures the Rust binary is built from source and
// points server-binary.json to the local target/release/goose binary.
// Rebuilds if source files are newer than the binary.

import { writeFileSync, existsSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, "..", "..", "..");
const binaryName = process.platform === "win32" ? "goose.exe" : "goose";
const binaryPath = join(projectRoot, "target", "release", binaryName);

// Verify we're in a development environment with Cargo.toml
const cargoToml = join(projectRoot, "Cargo.toml");
if (!existsSync(cargoToml)) {
  console.error("Error: Not in a Rust workspace (Cargo.toml not found)");
  console.error("This script is for development only. In production, use the prebuilt binaries.");
  process.exit(1);
}

function needsRebuild() {
  if (!existsSync(binaryPath)) {
    console.log("Binary not found, needs build");
    return true;
  }

  const binaryMtime = statSync(binaryPath).mtimeMs;
  
  // Check if any Rust source files are newer than the binary
  const cargoLock = join(projectRoot, "Cargo.lock");
  
  if (existsSync(cargoToml) && statSync(cargoToml).mtimeMs > binaryMtime) {
    console.log("Cargo.toml changed, needs rebuild");
    return true;
  }
  
  if (existsSync(cargoLock) && statSync(cargoLock).mtimeMs > binaryMtime) {
    console.log("Cargo.lock changed, needs rebuild");
    return true;
  }

  // Check goose-cli crate (the main binary)
  const cliSrc = join(projectRoot, "crates", "goose-cli", "src");
  if (existsSync(cliSrc)) {
    const cliCargoToml = join(projectRoot, "crates", "goose-cli", "Cargo.toml");
    if (existsSync(cliCargoToml) && statSync(cliCargoToml).mtimeMs > binaryMtime) {
      console.log("goose-cli Cargo.toml changed, needs rebuild");
      return true;
    }
  }

  return false;
}

function buildBinary() {
  console.log("Building goose-cli from source...");
  const result = spawnSync(
    "cargo",
    ["build", "--release", "-p", "goose-cli"],
    {
      cwd: projectRoot,
      stdio: "inherit",
    }
  );

  if (result.error) {
    console.error(`Failed to build: ${result.error.message}`);
    process.exit(1);
  }

  if (result.status !== 0) {
    console.error(`Build failed with exit code ${result.status}`);
    process.exit(1);
  }

  console.log(`Built goose binary at ${binaryPath}`);
}

// Main logic
if (needsRebuild()) {
  buildBinary();
} else {
  console.log("Binary is up to date, skipping build");
}

// Write the server-binary.json to point to the local build
const outDir = join(__dirname, "..");
writeFileSync(
  join(outDir, "server-binary.json"),
  JSON.stringify({ binaryPath }, null, 2) + "\n",
);

console.log(`Using local goose binary at ${binaryPath}`);
