import { INCLUDE_RE } from "./includePattern";

/** Build the editor text from separate workingDirs + prompt. */
export function buildEditorText(workingDirs: string[], prompt: string): string {
  const includeLines = workingDirs.map((directory) => `include: ${directory}`);
  if (includeLines.length === 0) return prompt;
  if (prompt === "") return includeLines.join("\n");
  return [prompt, "", ...includeLines].join("\n");
}

/** Parse editor text into { prompt, workingDirs }.
 *  `include:` lines can appear anywhere in the editor text and are
 *  extracted into workingDirs when the project is saved. */
export function parseEditorText(text: string): {
  prompt: string;
  workingDirs: string[];
} {
  const lines = text.split("\n");
  const workingDirs: string[] = [];
  const promptLines: string[] = [];

  for (const line of lines) {
    const match = line.match(INCLUDE_RE);
    if (match) {
      workingDirs.push(match[1].trim());
      continue;
    }

    promptLines.push(line);
  }

  // Trim leading/trailing blank lines from the prompt.
  while (promptLines[0]?.trim() === "") {
    promptLines.shift();
  }
  while (promptLines[promptLines.length - 1]?.trim() === "") {
    promptLines.pop();
  }

  return {
    prompt: promptLines.join("\n"),
    workingDirs,
  };
}

export function insertWorkingDir(text: string, directory: string): string {
  const trimmedText = text.trimEnd();

  if (trimmedText === "") {
    return `include: ${directory}`;
  }

  const trimmedLines = trimmedText.split("\n");
  const lastLine = trimmedLines[trimmedLines.length - 1] ?? "";
  const separator = lastLine.match(INCLUDE_RE) ? "\n" : "\n\n";

  return `${trimmedText}${separator}include: ${directory}`;
}

export function hasEquivalentWorkingDir(
  text: string,
  directory: string,
  homeDir: string | null,
): boolean {
  const normalizedDirectory = normalizeWorkingDirPath(directory, homeDir);

  return parseEditorText(text).workingDirs.some(
    (existingDirectory) =>
      normalizeWorkingDirPath(existingDirectory, homeDir) ===
      normalizedDirectory,
  );
}

function normalizeWorkingDirPath(
  directory: string,
  homeDir: string | null,
): string {
  const trimmedDirectory = directory.trim();
  const trimmedHomeDir = homeDir?.trim().replace(/[\\/]+$/, "") ?? null;

  if (!trimmedHomeDir) {
    return trimmedDirectory.replace(/[\\/]+$/, "");
  }

  if (trimmedDirectory === "~") {
    return trimmedHomeDir;
  }

  if (trimmedDirectory.startsWith("~/") || trimmedDirectory.startsWith("~\\")) {
    return `${trimmedHomeDir}${trimmedDirectory.slice(1)}`.replace(
      /[\\/]+$/,
      "",
    );
  }

  return trimmedDirectory.replace(/[\\/]+$/, "");
}
