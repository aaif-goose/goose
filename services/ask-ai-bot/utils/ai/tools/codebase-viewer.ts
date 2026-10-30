import fs from "fs";
import path from "path";
import { getCodebaseDir, sourceUrl } from "../source";

function getCodeChunk(
  filePath: string,
  startLine: number = 0,
  lineCount: number = 200,
): {
  filePath: string;
  content: string;
  totalLines: number;
  githubUrl: string;
  startLine: number;
  endLine: number;
  truncated: boolean;
} {
  const baseDir = path.resolve(getCodebaseDir());
  const fullPath = path.resolve(path.join(baseDir, filePath));

  if (!fullPath.startsWith(baseDir + "/")) {
    throw new Error("Invalid file path - directory traversal not allowed");
  }

  if (!fs.existsSync(fullPath)) {
    throw new Error(`File not found: ${filePath}`);
  }

  const stat = fs.statSync(fullPath);
  if (stat.isDirectory()) {
    throw new Error(
      `Path is a directory, not a file: ${filePath}. Use search_codebase with scope to explore directories, or list_codebase_files to list directory contents.`,
    );
  }

  const content = fs.readFileSync(fullPath, "utf-8");
  const lines = content.split("\n");
  const totalLines = lines.length;

  if (startLine < 0 || startLine >= totalLines)
    throw new Error(`File has ${totalLines} lines; startLine is out of range.`);
  const actualStart = startLine;
  const actualEnd = Math.min(actualStart + lineCount, lines.length);
  const chunkLines = lines.slice(actualStart, actualEnd);

  const numberedContent = chunkLines
    .map((line, i) => `${actualStart + i + 1}: ${line}`)
    .join("\n");

  return {
    filePath,
    content: numberedContent,
    totalLines,
    startLine: actualStart + 1,
    endLine: actualEnd,
    truncated: actualEnd < totalLines,
    githubUrl: sourceUrl(
      filePath,
      actualStart > 0 ? actualStart + 1 : undefined,
    ),
  };
}

export function viewCodebaseFiles(
  filePaths: string | string[],
  startLine: number = 0,
  lineCount: number = 200,
) {
  const paths = Array.isArray(filePaths) ? filePaths : [filePaths];

  return paths.map((filePath) => getCodeChunk(filePath, startLine, lineCount));
}
