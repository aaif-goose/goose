#!/usr/bin/env node

// Resolves the path to the goose binary from the platform-specific
// optional dependency. Writes the result to a JSON file that the CLI reads at
// startup so it can spawn the server automatically.

import { writeFileSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

const PLATFORMS = {
  "darwin-arm64": "@aaif/goose-binary-darwin-arm64",
  "darwin-x64": "@aaif/goose-binary-darwin-x64",
  "linux-arm64": "@aaif/goose-binary-linux-arm64",
  "linux-x64": "@aaif/goose-binary-linux-x64",
  "win32-x64": "@aaif/goose-binary-win32-x64",
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORMS[key];

if (!pkg) {
  console.warn(
    `@aaif/goose: no prebuilt goose binary for ${key}. ` +
      `You will need to provide a server URL manually with --server.`,
  );
  process.exit(0);
}

let binaryPath;
try {
  // Resolve the package directory, then point at the binary inside it
  const pkgDir = dirname(require.resolve(`${pkg}/package.json`));
  const binName = process.platform === "win32" ? "goose.exe" : "goose";
  binaryPath = join(pkgDir, "bin", binName);
} catch {
  // The optional dependency wasn't installed (e.g. wrong platform). That's fine.
  console.warn(
    `@aaif/goose: optional dependency ${pkg} not installed. ` +
      `You will need to provide a server URL manually with --server.`,
  );
  process.exit(0);
}

const outDir = join(__dirname, "..");
mkdirSync(outDir, { recursive: true });
writeFileSync(
  join(outDir, "server-binary.json"),
  JSON.stringify({ binaryPath }, null, 2) + "\n",
);

console.log(`@aaif/goose: found native goose binary at ${binaryPath}`);
