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

function appendArtifactsSegment(path: string): string {
  return `${path.replace(/[\\/]+$/, "")}/artifacts`;
}

function resolveProjectArtifactRoots(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
): string[] {
  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);

  if (workingDirs.length > 0) {
    return workingDirs.map(appendArtifactsSegment);
  }

  const artifactsDir = trimValue(project?.artifactsDir);
  return artifactsDir ? [artifactsDir] : [];
}

export function getProjectArtifactRoots(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
): string[] {
  return resolveProjectArtifactRoots(project);
}

export function resolveProjectDefaultArtifactRoot(
  project: ProjectInfo | null | undefined,
): string | undefined {
  const workingDirs = (project?.workingDirs ?? [])
    .map((directory) => trimValue(directory))
    .filter((directory): directory is string => directory !== null);

  if (workingDirs.length > 0) {
    return appendArtifactsSegment(workingDirs[0]);
  }

  return trimValue(project?.artifactsDir) ?? undefined;
}

export async function defaultGlobalArtifactRoot(): Promise<string> {
  return (await resolvePath({ parts: ["~", ".goose", "artifacts"] })).path;
}

export function getProjectFolderOption(
  project: Pick<ProjectInfo, "workingDirs" | "artifactsDir"> | null | undefined,
): ProjectFolderOption[] {
  return resolveProjectArtifactRoots(project).map((d) => ({
    id: d,
    name: getProjectFolderName(d),
    path: d,
  }));
}

export function buildProjectSystemPrompt(
  project: ProjectInfo | null | undefined,
): string | undefined {
  if (!project) {
    return undefined;
  }

  const artifactDir = resolveProjectDefaultArtifactRoot(project);
  const settings: string[] = [`Project name: ${project.name}`];
  const description = trimValue(project.description);
  const workingDirs = (project.workingDirs ?? [])
    .map((d) => trimValue(d))
    .filter((d): d is string => d !== null);
  const prompt = trimValue(project.prompt);

  if (description) {
    settings.push(`Project description: ${description}`);
  }
  if (workingDirs.length > 0) {
    settings.push(`Working directories: ${workingDirs.join(", ")}`);
  }
  if (artifactDir) {
    settings.push(`Artifact directory: ${artifactDir}`);
  }
  if (project.preferredProvider) {
    settings.push(`Preferred provider: ${project.preferredProvider}`);
  }
  if (project.preferredModel) {
    settings.push(`Preferred model: ${project.preferredModel}`);
  }
  settings.push(
    `Use git worktrees for branch isolation: ${
      project.useWorktrees ? "yes" : "no"
    }`,
  );

  const sections = [
    `<project-settings>\n${settings.join("\n")}\n</project-settings>`,
  ];

  if (artifactDir) {
    sections.push(
      `<project-file-policy>\n` +
        `Write newly generated files to ${artifactDir} by default.\n` +
        `When creating translations, variants, summaries, or derived documents from existing project files, save the new file in ${artifactDir} instead of the project root.\n` +
        `Only write outside ${artifactDir} when the user explicitly asks you to edit or create a file at a specific path.\n` +
        `If you need to read existing files elsewhere in the project, that is fine, but generated outputs should stay in ${artifactDir} unless the user says otherwise.\n` +
        `</project-file-policy>`,
    );
  }

  if (prompt) {
    sections.push(`<project-instructions>\n${prompt}\n</project-instructions>`);
  }

  return sections.join("\n\n");
}

export function composeSystemPrompt(
  ...parts: Array<string | null | undefined>
): string | undefined {
  const combined = parts
    .map((part) => trimValue(part))
    .filter((part): part is string => part !== null);

  return combined.length > 0 ? combined.join("\n\n") : undefined;
}
