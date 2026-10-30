import fs from "fs";
import path from "path";

export function getCodebaseDir(): string {
  return path.resolve(
    process.env.CODEBASE_PATH || path.join(process.cwd(), "../.."),
  );
}

export function sourceUrl(filePath: string, line?: number): string {
  const revision = process.env.SOURCE_REVISION || "main";
  return `https://github.com/aaif-goose/goose/blob/${revision}/${filePath}${line ? `#L${line}` : ""}`;
}

export function sourceContext(): string {
  const manifest = path.join(getCodebaseDir(), "Cargo.toml");
  const version = fs.existsSync(manifest)
    ? fs.readFileSync(manifest, "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1]
    : undefined;
  return `Bundled docs and code are a development snapshot${version ? ` (package version ${version})` : ""}, revision ${process.env.SOURCE_REVISION || "unknown"}. They may describe unreleased changes. Check GitHub releases before claiming a fix or feature has shipped.`;
}
