import { create } from "zustand";
import {
  listProjects,
  createProject,
  updateProject,
  deleteProject,
  reorderProjects as apiReorderProjects,
  type ProjectInfo,
} from "../api/projects";

const PROJECT_CACHE_STORAGE_KEY = "goose:projects";

function loadCachedProjects(): ProjectInfo[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = window.localStorage.getItem(PROJECT_CACHE_STORAGE_KEY);
    if (!stored) return [];
    const parsed = JSON.parse(stored);
    return Array.isArray(parsed) ? (parsed as ProjectInfo[]) : [];
  } catch {
    return [];
  }
}

function persistProjects(projects: ProjectInfo[]): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(
      PROJECT_CACHE_STORAGE_KEY,
      JSON.stringify(projects),
    );
  } catch {
    // localStorage may be unavailable
  }
}

interface ProjectState {
  projects: ProjectInfo[];
  loading: boolean;
  activeProjectId: string | null;

  // Actions
  fetchProjects: () => Promise<void>;
  addProject: (
    name: string,
    description: string,
    prompt: string,
    icon: string,
    color: string,
    preferredProvider: string | null,
    preferredModel: string | null,
    workingDirs: string[],
    useWorktrees: boolean,
  ) => Promise<ProjectInfo>;
  editProject: (
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
  ) => Promise<ProjectInfo>;
  removeProject: (id: string) => Promise<void>;
  reorderProjects: (fromId: string, toId: string) => void;
  setActiveProject: (id: string | null) => void;
  getActiveProject: () => ProjectInfo | null;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projects: loadCachedProjects(),
  loading: false,
  activeProjectId: null,

  fetchProjects: async () => {
    set({ loading: true });
    try {
      const projects = await listProjects();
      set({ projects, loading: false });
      persistProjects(projects);
    } catch {
      set({ loading: false });
    }
  },

  addProject: async (
    name,
    description,
    prompt,
    icon,
    color,
    preferredProvider,
    preferredModel,
    workingDirs,
    useWorktrees,
  ) => {
    const project = await createProject(
      name,
      description,
      prompt,
      icon,
      color,
      preferredProvider,
      preferredModel,
      workingDirs,
      useWorktrees,
    );
    set((state) => ({ projects: [...state.projects, project] }));
    persistProjects(get().projects);
    return project;
  },

  editProject: async (
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
  ) => {
    const project = await updateProject(
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
    );
    set((state) => ({
      projects: state.projects.map((p) => (p.id === id ? project : p)),
    }));
    persistProjects(get().projects);
    return project;
  },

  removeProject: async (id) => {
    await deleteProject(id);
    set((state) => ({
      projects: state.projects.filter((p) => p.id !== id),
      activeProjectId:
        state.activeProjectId === id ? null : state.activeProjectId,
    }));
    persistProjects(get().projects);
  },

  reorderProjects: (fromId, toId) => {
    set((state) => {
      const projects = [...state.projects];
      const fromIndex = projects.findIndex((p) => p.id === fromId);
      const toIndex = projects.findIndex((p) => p.id === toId);
      if (fromIndex === -1 || toIndex === -1 || fromIndex === toIndex)
        return state;
      const [moved] = projects.splice(fromIndex, 1);
      // When dragging down, removing the source shifts the target index
      const insertAt = fromIndex < toIndex ? toIndex - 1 : toIndex;
      projects.splice(insertAt, 0, moved);
      // Update order fields so views sorting by .order stay consistent
      for (let i = 0; i < projects.length; i++) {
        projects[i] = { ...projects[i], order: i };
      }
      return { projects };
    });
    const projects = get().projects;
    persistProjects(projects);
    void apiReorderProjects(projects.map((p, i) => [p.id, i]));
  },

  setActiveProject: (id) => set({ activeProjectId: id }),

  getActiveProject: () => {
    const { projects, activeProjectId } = get();
    return projects.find((p) => p.id === activeProjectId) ?? null;
  },
}));
