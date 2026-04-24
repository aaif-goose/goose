import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const PLATFORMS: Record<string, string> = {
  "darwin-arm64": "@aaif/goose-binary-darwin-arm64",
  "darwin-x64": "@aaif/goose-binary-darwin-x64",
  "linux-arm64": "@aaif/goose-binary-linux-arm64",
  "linux-x64": "@aaif/goose-binary-linux-x64",
  "win32-x64": "@aaif/goose-binary-win32-x64",
};

/**
 * Resolves the path to the goose binary.
 *
 * Resolution order:
 *   1. `GOOSE_BINARY` environment variable (explicit override)
 *   2. Platform-specific `@aaif/goose-binary-*` optional dependency
 *
 * @throws if no binary can be found
 */
export function resolveGooseBinary(): string {
  const envBinary = process.env.GOOSE_BINARY;
  if (envBinary) return envBinary;

  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORMS[key];
  if (!pkg) {
    throw new Error(
      `No goose binary available for ${key}. Set GOOSE_BINARY to the path of a goose binary.`,
    );
  }

  try {
    const require = createRequire(import.meta.url);
    const pkgDir = dirname(require.resolve(`${pkg}/package.json`));
    const binName = process.platform === "win32" ? "goose.exe" : "goose";
    return join(pkgDir, "bin", binName);
  } catch {
    throw new Error(
      `goose binary package ${pkg} is not installed. Set GOOSE_BINARY or install the native package.`,
    );
  }
}
