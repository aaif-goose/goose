#!/usr/bin/env node
/**
 * Builds the generate-acp-schema Rust binary and runs it to generate
 * acp-schema.json and acp-meta.json, then generates TypeScript types.
 *
 * Usage:
 *   npm run build:schema
 */

import { execSync } from "child_process";
import { dirname, resolve } from "path";
import { fileURLToPath } from "url";
import { existsSync, copyFileSync, mkdirSync } from "fs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = resolve(__dirname, "../../..");
const ACP_CRATE = resolve(ROOT, "crates/goose-acp");
const SCHEMA_PATH = resolve(ACP_CRATE, "acp-schema.json");
const META_PATH = resolve(ACP_CRATE, "acp-meta.json");
const LOCAL_SCHEMA_PATH = resolve(__dirname, "..", "acp-schema.json");
const LOCAL_META_PATH = resolve(__dirname, "..", "acp-meta.json");

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

async function main() {
  console.log("==> Building generate-acp-schema binary...");
  
  try {
    execSync(
      "cargo build --release --bin generate-acp-schema",
      {
        cwd: ROOT,
        stdio: "inherit",
      }
    );
  } catch (err) {
    console.error("Failed to build generate-acp-schema binary");
    throw err;
  }

  console.log("==> Running generate-acp-schema...");
  
  try {
    execSync(
      "cargo run --release --bin generate-acp-schema",
      {
        cwd: ACP_CRATE,
        stdio: "inherit",
      }
    );
  } catch (err) {
    console.error("Failed to generate schema");
    throw err;
  }

  // Copy schema files to ui/acp for reference
  console.log("==> Copying schema files to ui/acp...");
  mkdirSync(dirname(LOCAL_SCHEMA_PATH), { recursive: true });
  copyFileSync(SCHEMA_PATH, LOCAL_SCHEMA_PATH);
  copyFileSync(META_PATH, LOCAL_META_PATH);

  console.log("==> Generating TypeScript types...");
  
  // Import and run the generate-schema logic
  const { default: generateSchema } = await import("../generate-schema.js");
  await generateSchema();

  console.log("✅ Schema generation complete");
}
