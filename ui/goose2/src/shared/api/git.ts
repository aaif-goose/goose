import { invoke } from "@tauri-apps/api/core";
import type {
  ChangedFile,
  CreatedWorktree,
  GitState,
} from "@/shared/types/git";

export async function getGitState(path: string): Promise<GitState> {
  return invoke("get_git_state", { path });
}

export async function switchBranch(
  path: string,
  branch: string,
): Promise<void> {
  return invoke("git_switch_branch", { path, branch });
}

export async function stashChanges(path: string): Promise<void> {
  return invoke("git_stash", { path });
}

export async function initRepo(path: string): Promise<void> {
  return invoke("git_init", { path });
}

export async function fetchRepo(path: string): Promise<void> {
  return invoke("git_fetch", { path });
}

export async function pullRepo(path: string): Promise<void> {
  return invoke("git_pull", { path });
}

export async function createBranch(
  path: string,
  name: string,
  baseBranch: string,
): Promise<void> {
  return invoke("git_create_branch", { path, name, baseBranch });
}

export async function getChangedFiles(path: string): Promise<ChangedFile[]> {
  return invoke("get_changed_files", { path });
}

export async function createWorktree(
  path: string,
  name: string,
  branch: string,
  createBranch: boolean,
  baseBranch?: string,
): Promise<CreatedWorktree> {
  return invoke("git_create_worktree", {
    path,
    name,
    branch,
    createBranch,
    baseBranch,
  });
}
