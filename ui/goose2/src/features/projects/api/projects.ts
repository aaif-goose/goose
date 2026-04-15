import { invoke } from "@tauri-apps/api/core";

export interface ProjectInfo {
  id: string;
  name: string;
  description: string;
  prompt: string;
  icon: string;
  color: string;
  preferredProvider: string | null;
  preferredModel: string | null;
  workingDirs: string[];
  useWorktrees: boolean;
  order: number;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
  artifactsDir: string;
}

export async function listProjects(): Promise<ProjectInfo[]> {
  return invoke("list_projects");
}

export async function createProject(
  name: string,
  description: string,
  prompt: string,
  icon: string,
  color: string,
  preferredProvider: string | null,
  preferredModel: string | null,
  workingDirs: string[],
  useWorktrees: boolean,
): Promise<ProjectInfo> {
  return invoke("create_project", {
    name,
    description,
    prompt,
    icon,
    color,
    preferredProvider,
    preferredModel,
    workingDirs,
    useWorktrees,
  });
}

export async function updateProject(
  id: string,
  name: string,
  description: string,
  prompt: string,
  icon: string,
  color: string,
  preferredProvider: string | null,
  preferredModel: string | null,
  workingDirs: string[],
  useWorktrees: boolean,
): Promise<ProjectInfo> {
  return invoke("update_project", {
    id,
    name,
    description,
    prompt,
    icon,
    color,
    preferredProvider,
    preferredModel,
    workingDirs,
    useWorktrees,
  });
}

export async function deleteProject(id: string): Promise<void> {
  return invoke("delete_project", { id });
}

export async function getProject(id: string): Promise<ProjectInfo> {
  return invoke("get_project", { id });
}

export async function listArchivedProjects(): Promise<ProjectInfo[]> {
  return invoke("list_archived_projects");
}

export async function archiveProject(id: string): Promise<void> {
  return invoke("archive_project", { id });
}

export async function restoreProject(id: string): Promise<void> {
  return invoke("restore_project", { id });
}
