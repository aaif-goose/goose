import type { ProjectInfo } from "../api/projects";
import { resolvePath } from "@/shared/api/pathResolver";

function trimValue(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function buildSessionCwdParts(
  project: ProjectInfo | null | undefined,
  activeWorkspacePath?: string | null,
): string[] {
  const trimmedWorkspacePath = trimValue(activeWorkspacePath);
  if (trimmedWorkspacePath) {
    return [trimmedWorkspacePath];
  }

  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);
  if (workingDirs.length > 0) {
    return [workingDirs[0]];
  }

  return ["~"];
}

export async function resolveSessionCwd(
  project: ProjectInfo | null | undefined,
  activeWorkspacePath?: string | null,
): Promise<string> {
  return (
    await resolvePath({
      parts: buildSessionCwdParts(project, activeWorkspacePath),
    })
  ).path;
}
