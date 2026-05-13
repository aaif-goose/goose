const PROJECT_SKILLS_MARKERS = [
  "/.agents/skills/",
  "/.goose/skills/",
  "/.claude/skills/",
];

export function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

export function basename(path: string): string {
  const trimmed = normalizePath(path).replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}

export function getSkillFileLocation(directory: string): string {
  const separator = directory.includes("\\") ? "\\" : "/";
  return directory.endsWith(separator)
    ? `${directory}SKILL.md`
    : `${directory}${separator}SKILL.md`;
}

export function deriveProjectRoot(directory: string): string | null {
  const normalizedDirectory = normalizePath(directory);

  for (const marker of PROJECT_SKILLS_MARKERS) {
    const idx = normalizedDirectory.lastIndexOf(marker);
    if (idx >= 0) {
      return directory.slice(0, idx);
    }
  }

  return null;
}

export function getRenamedSkillFileLocation(
  fileLocation: string,
  name: string,
): string {
  const separator = fileLocation.includes("\\") ? "\\" : "/";
  const parts = fileLocation.split(separator);

  if (parts.length >= 2) {
    parts[parts.length - 2] = name;
  }

  return parts.join(separator);
}
