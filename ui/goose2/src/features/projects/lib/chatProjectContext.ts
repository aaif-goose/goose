import type { ProjectInfo } from "../api/projects";
import { resolvePath } from "@/shared/api/pathResolver";

export interface ProjectFolderOption {
  id: string;
  name: string;
  path?: string;
}

function trimValue(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

export function getProjectFolderName(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  if (!normalized) {
    return path;
  }

  const parts = normalized.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function resolveProjectRoots(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string[] {
  return (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);
}

export function getProjectArtifactRoots(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string[] {
  return resolveProjectRoots(project);
}

export function resolveProjectDefaultArtifactRoot(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): string | undefined {
  const workingDirs = resolveProjectRoots(project);
  return workingDirs[0];
}

export async function defaultGlobalArtifactRoot(): Promise<string> {
  return (await resolvePath({ parts: ["~"] })).path;
}

export function getProjectFolderOption(
  project: Pick<ProjectInfo, "workingDirs"> | null | undefined,
): ProjectFolderOption[] {
  return resolveProjectRoots(project).map((d) => ({
    id: d,
    name: getProjectFolderName(d),
    path: d,
  }));
}

export function composeSystemPrompt(
  ...parts: Array<string | null | undefined>
): string | undefined {
  const combined = parts
    .map((part) => trimValue(part))
    .filter((part): part is string => part !== null);

  return combined.length > 0 ? combined.join("\n\n") : undefined;
}
