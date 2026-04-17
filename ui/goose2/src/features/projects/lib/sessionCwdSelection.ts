import type { ProjectInfo } from "../api/projects";
import { resolveOptionalPath } from "@/shared/api/pathResolver";

function trimValue(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function appendArtifactsSegment(path: string): string {
  return `${path.replace(/[\\/]+$/, "")}/artifacts`;
}

export function defaultGlobalArtifactRoot(homeDir: string): string {
  const normalizedHome = homeDir.replace(/\\/g, "/").replace(/\/+$/, "");
  return `${normalizedHome}/.goose/artifacts`;
}

export function resolveProjectDefaultArtifactRoot(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
): string | undefined {
  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);

  if (workingDirs.length > 0) {
    return appendArtifactsSegment(workingDirs[0]);
  }

  return trimValue(project?.artifactsDir) ?? undefined;
}

export function resolveDefaultSessionCwd(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
  homeDir?: string,
): string | undefined {
  const projectArtifactRoot = resolveProjectDefaultArtifactRoot(project);
  if (projectArtifactRoot) {
    return projectArtifactRoot;
  }
  if (project) {
    return undefined;
  }
  if (!homeDir) {
    return undefined;
  }
  return defaultGlobalArtifactRoot(homeDir);
}

export function buildSessionCwdParts(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
  activeWorkspacePath?: string | null,
): string[] | undefined {
  const trimmedWorkspacePath = trimValue(activeWorkspacePath);
  if (trimmedWorkspacePath) {
    return [trimmedWorkspacePath];
  }

  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);
  if (workingDirs.length > 0) {
    return [workingDirs[0], "artifacts"];
  }

  const artifactRoot = trimValue(project?.artifactsDir);
  if (artifactRoot) {
    return [artifactRoot];
  }

  if (project) {
    return undefined;
  }

  return ["~", ".goose", "artifacts"];
}

export async function resolveSessionCwd(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
  activeWorkspacePath?: string | null,
): Promise<string | undefined> {
  return resolveOptionalPath(
    buildSessionCwdParts(project, activeWorkspacePath),
  );
}
