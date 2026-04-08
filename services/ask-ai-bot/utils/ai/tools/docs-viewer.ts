import fs from "fs";
import path from "path";

function getDocsDir(): string {
  return process.env.DOCS_PATH || path.join(process.cwd(), "docs");
}

function generateWebUrl(filePath: string): string {
  const baseUrl = "https://goose-docs.ai/docs";
  // Remove file extension for the URL path
  const urlPath = filePath.replace(/\.[^/.]+$/, "");
  return `${baseUrl}/${urlPath}`;
}

function findDocFile(partialPath: string): string | null {
  const docsDir = getDocsDir();

  if (!fs.existsSync(docsDir)) {
    return null;
  }

  const searchTerm = partialPath.toLowerCase().replace(/\.md$/, "");
  let foundPath: string | null = null;

  function walkDir(dir: string) {
    if (foundPath) return;

    const files = fs.readdirSync(dir);

    for (const file of files) {
      const filePath = path.join(dir, file);
      const stat = fs.statSync(filePath);

      if (stat.isDirectory()) {
        if (file === "assets" || file === "docker") {
          continue;
        }
        walkDir(filePath);
      } else {
        const relativePath = path.relative(docsDir, filePath);
        if (relativePath.toLowerCase().includes(searchTerm)) {
          foundPath = relativePath;
          return;
        }
      }
    }
  }

  walkDir(docsDir);
  return foundPath;
}

function getDocChunk(
  filePath: string,
  startLine: number = 0,
  lineCount: number = 1500,
): { fileName: string; content: string; webUrl: string } {
  const docsDir = path.resolve(getDocsDir());
  const fullPath = path.join(docsDir, filePath);

  const normalizedPath = path.resolve(fullPath);
  if (!normalizedPath.startsWith(docsDir + "/")) {
    throw new Error("Invalid file path - directory traversal not allowed");
  }

  if (!fs.existsSync(fullPath)) {
    throw new Error(`Documentation file not found: ${filePath}`);
  }

  try {
    const content = fs.readFileSync(fullPath, "utf-8");
    const lines = content.split("\n");

    const actualStartLine = Math.max(0, Math.min(startLine, lines.length - 1));
    const actualEndLine = Math.min(actualStartLine + lineCount, lines.length);
    const chunkLines = lines.slice(actualStartLine, actualEndLine);
    const chunkContent = chunkLines.join("\n");

    const fileName = path.basename(fullPath);

    return {
      content: chunkContent,
      fileName,
      webUrl: generateWebUrl(filePath),
    };
  } catch (error) {
    if (error instanceof Error && error.message.includes("ENOENT")) {
      throw new Error(`Documentation file not found: ${filePath}`);
    }
    throw error;
  }
}

export function viewDocs(
  filePaths: string | string[],
  startLine: number = 0,
  lineCount: number = 1500,
): string {
  const paths = Array.isArray(filePaths) ? filePaths : [filePaths];

  const docs = paths.map((filePath) => {
    let resolvedPath = filePath;
    // Check if file has an extension; if not, search for it
    if (!path.extname(filePath)) {
      const found = findDocFile(filePath);
      if (found) {
        resolvedPath = found;
      }
    }
    return getDocChunk(resolvedPath, startLine, lineCount);
  });

  return docs
    .map(
      (doc) =>
        `**${doc.fileName}**\nWeb URL: <${doc.webUrl}>\n\`\`\`\n${doc.content}\n\`\`\``,
    )
    .join("\n\n---\n\n");
}
