import fs from "fs";
import { readdir, readFile } from "fs/promises";
import path from "path";
import { getCodebaseDir } from "../source";

export interface CodeSearchResult {
  filePath: string;
  line: number;
  content: string;
  context: string;
}

const SOURCE_EXTENSIONS = new Set([
  ".rs",
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".json",
  ".toml",
  ".yaml",
  ".yml",
  ".css",
  ".scss",
  ".html",
  ".md",
  ".sql",
  ".sh",
  ".mts",
]);

const IGNORED_DIRS = new Set([
  "node_modules",
  "target",
  "dist",
  "out",
  ".vite",
  ".git",
  "build",
  "coverage",
]);

function getSearchableDirs(): { name: string; path: string }[] {
  const base = path.resolve(getCodebaseDir());
  return [
    { name: "ui", path: path.join(base, "ui") },
    { name: "crates", path: path.join(base, "crates") },
  ];
}

function shouldSkipDir(dirName: string): boolean {
  return IGNORED_DIRS.has(dirName);
}

function isSourceFile(fileName: string): boolean {
  const ext = path.extname(fileName).toLowerCase();
  return SOURCE_EXTENSIONS.has(ext);
}

function getContextLines(
  lines: string[],
  matchLine: number,
  contextSize: number = 2,
): string {
  const start = Math.max(0, matchLine - contextSize);
  const end = Math.min(lines.length - 1, matchLine + contextSize);
  const contextLines: string[] = [];

  for (let i = start; i <= end; i++) {
    const prefix = i === matchLine ? ">" : " ";
    contextLines.push(`${prefix} ${i + 1}: ${lines[i]}`);
  }

  return contextLines.join("\n");
}

async function searchInFile(
  filePath: string,
  pattern: RegExp,
  baseDir: string,
): Promise<CodeSearchResult[]> {
  const results: CodeSearchResult[] = [];

  try {
    const content = await readFile(filePath, "utf-8");
    const lines = content.split("\n");

    for (let i = 0; i < lines.length && results.length < 3; i++) {
      if (pattern.test(lines[i])) {
        const relativePath = path.relative(baseDir, filePath);
        results.push({
          filePath: relativePath,
          line: i + 1,
          content: lines[i].trim(),
          context: getContextLines(lines, i),
        });
        i += 4;
      }
    }
  } catch {
    // Skip files that can't be read (binary, permissions, etc.)
  }

  return results;
}

async function walkAndSearch(
  dir: string,
  pattern: RegExp,
  baseDir: string,
  results: CodeSearchResult[],
  maxResults: number,
): Promise<void> {
  if (results.length >= maxResults) return;

  const entries = await readdir(dir, { withFileTypes: true });

  for (const entry of entries) {
    if (results.length >= maxResults) return;

    if (entry.isDirectory()) {
      if (shouldSkipDir(entry.name)) continue;
      await walkAndSearch(
        path.join(dir, entry.name),
        pattern,
        baseDir,
        results,
        maxResults,
      );
    } else if (entry.isFile() && isSourceFile(entry.name)) {
      const fileResults = await searchInFile(
        path.join(dir, entry.name),
        pattern,
        baseDir,
      );
      for (const result of fileResults) {
        if (results.length >= maxResults) return;
        results.push(result);
      }
    }
  }
}

export async function searchCodebase(
  query: string,
  limit: number = 20,
  scope?: string,
  directory?: string,
  regex = false,
): Promise<CodeSearchResult[]> {
  const base = getCodebaseDir();
  let searchDirs = getSearchableDirs().filter(
    (dir) => !scope || dir.name === scope,
  );
  if (directory) {
    const target = path.resolve(base, directory);
    if (
      !searchDirs.some(
        (dir) => target === dir.path || target.startsWith(dir.path + path.sep),
      )
    )
      throw new Error(
        "Search directory must be within the selected ui/ or crates/ scope.",
      );
    searchDirs = [{ name: directory, path: target }];
  }
  const pattern = new RegExp(
    regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    "i",
  );
  const groups = await Promise.all(
    searchDirs.map(async (dir) => {
      const results: CodeSearchResult[] = [];
      await walkAndSearch(dir.path, pattern, base, results, limit);
      return results;
    }),
  );
  const results: CodeSearchResult[] = [];
  for (
    let i = 0;
    results.length < limit && groups.some((group) => i < group.length);
    i++
  ) {
    for (const group of groups) {
      if (group[i] && results.length < limit) results.push(group[i]);
    }
  }
  return results;
}

export function listCodebaseFiles(
  directory: string,
): { filePath: string; isDirectory: boolean }[] {
  const baseDir = path.resolve(getCodebaseDir());
  const targetDir = path.resolve(path.join(baseDir, directory));

  if (targetDir !== baseDir && !targetDir.startsWith(baseDir + "/")) {
    throw new Error("Invalid path - directory traversal not allowed");
  }

  if (!fs.existsSync(targetDir)) {
    throw new Error(`Directory not found: ${directory}`);
  }

  const stat = fs.statSync(targetDir);
  if (!stat.isDirectory()) {
    throw new Error(`Not a directory: ${directory}`);
  }

  try {
    const entries = fs.readdirSync(targetDir, { withFileTypes: true });
    return entries
      .filter((entry) => !shouldSkipDir(entry.name))
      .map((entry) => ({
        filePath: path.join(directory, entry.name),
        isDirectory: entry.isDirectory(),
      }))
      .sort((a, b) => {
        if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1;
        return a.filePath.localeCompare(b.filePath);
      });
  } catch (error) {
    throw new Error(`Failed to list directory: ${directory}`);
  }
}
