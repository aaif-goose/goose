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
}

export interface ProjectIconCandidate {
  id: string;
  label: string;
  icon: string;
  sourceDir: string;
}

export interface ProjectIconData {
  icon: string;
}

export async function listProjects(): Promise<ProjectInfo[]> {
  return invoke("list_projects");
}

export async function scanProjectIcons(
  workingDirs: string[],
): Promise<ProjectIconCandidate[]> {
  return invoke("scan_project_icons", { workingDirs });
}

export async function readProjectIcon(path: string): Promise<ProjectIconData> {
  return invoke("read_project_icon", { path });
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

export async function reorderProjects(
  order: [string, number][],
): Promise<void> {
  return invoke("reorder_projects", { order });
}

export async function restoreProject(id: string): Promise<void> {
  return invoke("restore_project", { id });
}
