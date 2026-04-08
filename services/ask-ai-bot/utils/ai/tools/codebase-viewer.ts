import fs from "fs";
import path from "path";

const GITHUB_BASE_URL = "https://github.com/aaif-goose/goose/blob/main";

function getCodebaseDir(): string {
  return process.env.CODEBASE_PATH || path.join(process.cwd(), "../..");
}

function generateGitHubUrl(filePath: string, startLine?: number): string {
  const url = `${GITHUB_BASE_URL}/${filePath}`;
  if (startLine && startLine > 0) {
    return `${url}#L${startLine}`;
  }
  return url;
}

function getCodeChunk(
  filePath: string,
  startLine: number = 0,
  lineCount: number = 200,
): {
  filePath: string;
  content: string;
  totalLines: number;
  githubUrl: string;
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

  const actualStart = Math.max(0, Math.min(startLine, lines.length - 1));
  const actualEnd = Math.min(actualStart + lineCount, lines.length);
  const chunkLines = lines.slice(actualStart, actualEnd);

  const numberedContent = chunkLines
    .map((line, i) => `${actualStart + i + 1}: ${line}`)
    .join("\n");

  return {
    filePath,
    content: numberedContent,
    totalLines,
    githubUrl: generateGitHubUrl(
      filePath,
      actualStart > 0 ? actualStart + 1 : undefined,
    ),
  };
}

export function viewCodebaseFiles(
  filePaths: string | string[],
  startLine: number = 0,
  lineCount: number = 200,
): string {
  const paths = Array.isArray(filePaths) ? filePaths : [filePaths];

  const results = paths.map((filePath) => {
    const chunk = getCodeChunk(filePath, startLine, lineCount);
    const ext = path.extname(filePath).slice(1) || "text";
    const lineInfo =
      startLine > 0
        ? ` (lines ${startLine + 1}-${Math.min(startLine + lineCount, chunk.totalLines)} of ${chunk.totalLines})`
        : ` (${chunk.totalLines} lines total)`;

    return `**${chunk.filePath}**${lineInfo}\nGitHub: <${chunk.githubUrl}>\n\`\`\`${ext}\n${chunk.content}\n\`\`\``;
  });

  return results.join("\n\n---\n\n");
}
