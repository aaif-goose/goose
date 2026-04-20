import { invoke } from "@tauri-apps/api/core";

export interface FolderNote {
  id: string;
  title: string;
  path: string;
  kind: "note" | "wiki";
  updatedMs: number | null;
  bytes: number;
}

export interface FolderProject {
  id: string;
  name: string;
  path: string;
  noteCount: number;
}

export async function listMemoryNotes(dir: string): Promise<FolderNote[]> {
  return invoke<FolderNote[]>("list_memory_notes", { dir });
}

export async function readNote(path: string): Promise<string> {
  return invoke<string>("read_note", { path });
}

export async function listProjects(dir: string): Promise<FolderProject[]> {
  return invoke<FolderProject[]>("list_projects", { dir });
}

export async function listProjectNotes(projectPath: string): Promise<FolderNote[]> {
  return invoke<FolderNote[]>("list_project_notes", { projectPath });
}

export function formatRelativeTime(ms: number | null): string {
  if (!ms) return "";
  const diffSec = Math.max(0, (Date.now() - ms) / 1000);
  if (diffSec < 60) return "just now";
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  const days = Math.floor(diffSec / 86400);
  if (days < 7) return `${days}d ago`;
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
