#!/usr/bin/env node

// Development entrypoint: ensures a goose binary is available, then launches the TUI
// Skips the cargo build if GOOSE_BINARY is already set or if --server is provided

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..", "..", "..");
const args = process.argv.slice(2);
const hasServerFlag = args.some(
  (arg) =>
    arg === "--server" ||
    arg === "-s" ||
    arg.startsWith("--server=") ||
    arg.startsWith("-s="),
);

if (!hasServerFlag && !process.env.GOOSE_BINARY) {
  const binName = process.platform === "win32" ? "goose.exe" : "goose";
  const binaryPath = join(repoRoot, "target", "debug", binName);

  console.log("Building goose (debug)…");
  execFileSync("cargo", ["build", "-p", "goose-cli"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  if (!existsSync(binaryPath)) {
    console.error(`Build succeeded but binary not found at ${binaryPath}`);
    process.exit(1);
  }

  process.env.GOOSE_BINARY = binaryPath;
}

execFileSync("tsx", [join(__dirname, "..", "src", "tui.tsx"), ...process.argv.slice(2)], {
  cwd: process.cwd(),
  stdio: "inherit",
  env: process.env,
});
