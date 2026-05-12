import { invoke } from "@tauri-apps/api/core";
import { getClient } from "@/shared/api/acpConnection";

export interface ProjectInfo {
  id: string;
  /** Stable on-disk path of the project source. Pass back to update/delete. */
  path: string;
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
}

// Shape returned by _goose/sources/*. Narrowed to project-type sources here.
interface SourceEntry {
  type: "project";
  name: string;
  description: string;
  content: string;
  path: string;
  global: boolean;
  properties: Record<string, unknown>;
}

function toProjectInfo(source: SourceEntry): ProjectInfo {
  const p = source.properties ?? {};
  return {
    id: source.name,
    path: source.path,
    name: (p.title as string) ?? source.name,
    description: source.description,
    prompt: source.content,
    icon: (p.icon as string) ?? "",
    color: (p.color as string) ?? "",
    preferredProvider: (p.preferredProvider as string) ?? null,
    preferredModel: (p.preferredModel as string) ?? null,
    workingDirs: (p.workingDirs as string[]) ?? [],
    useWorktrees: (p.useWorktrees as boolean) ?? false,
    order: (p.order as number) ?? 0,
    archivedAt: (p.archivedAt as string) ?? null,
  };
}

interface ProjectMetadataFields {
  name: string;
  icon: string;
  color: string;
  preferredProvider: string | null;
  preferredModel: string | null;
  workingDirs: string[];
  useWorktrees: boolean;
  order: number;
  archivedAt: string | null;
}

function toProperties(info: ProjectMetadataFields): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  if (info.name) props.title = info.name;
  if (info.icon) props.icon = info.icon;
  if (info.color) props.color = info.color;
  if (info.preferredProvider) props.preferredProvider = info.preferredProvider;
  if (info.preferredModel) props.preferredModel = info.preferredModel;
  if (info.workingDirs?.length) props.workingDirs = info.workingDirs;
  if (info.useWorktrees) props.useWorktrees = info.useWorktrees;
  if (typeof info.order === "number") props.order = info.order;
  if (info.archivedAt) props.archivedAt = info.archivedAt;
  return props;
}

function slugify(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "project";
}

/** Pick a slug for `name` that does not collide with any existing project ID
 *  (active or archived). Two display names that normalize to the same slug
 *  (e.g. "My App" and "my-app", or both collapsing to "project" because they
 *  contain no ASCII alphanumerics) are disambiguated with a numeric suffix. */
function uniqueProjectSlug(name: string, existingIds: Set<string>): string {
  const base = slugify(name);
  if (!existingIds.has(base)) {
    return base;
  }
  let counter = 2;
  while (existingIds.has(`${base}-${counter}`)) {
    counter += 1;
  }
  return `${base}-${counter}`;
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
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources
    .map(toProjectInfo)
    .filter((p) => p.archivedAt === null)
    .sort((a, b) => a.order - b.order);
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
  const client = await getClient();
  const existing = await listAllProjects();
  const id = uniqueProjectSlug(name, new Set(existing.map((p) => p.id)));
  const raw = await client.extMethod("_goose/sources/create", {
    type: "project",
    name: id,
    description,
    content: prompt,
    global: true,
    properties: toProperties({
      name,
      icon,
      color,
      preferredProvider,
      preferredModel,
      workingDirs,
      useWorktrees,
      order: 0,
      archivedAt: null,
    }),
  });
  return toProjectInfo(raw.source as SourceEntry);
}

export async function updateProject(
  existing: ProjectInfo,
  updates: Partial<Omit<ProjectInfo, "id" | "path">>,
): Promise<ProjectInfo> {
  const merged = { ...existing, ...updates };
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/update", {
    type: "project",
    path: existing.path,
    name: existing.id,
    description: merged.description,
    content: merged.prompt,
    properties: toProperties({
      name: merged.name,
      icon: merged.icon,
      color: merged.color,
      preferredProvider: merged.preferredProvider,
      preferredModel: merged.preferredModel,
      workingDirs: merged.workingDirs,
      useWorktrees: merged.useWorktrees,
      order: merged.order,
      archivedAt: merged.archivedAt,
    }),
  });
  return toProjectInfo(raw.source as SourceEntry);
}

export async function deleteProject(
  idOrProject: string | ProjectInfo,
): Promise<void> {
  const client = await getClient();
  const path =
    typeof idOrProject === "string"
      ? (await getProject(idOrProject)).path
      : idOrProject.path;
  await client.extMethod("_goose/sources/delete", {
    type: "project",
    path,
  });
}

export async function getProject(id: string): Promise<ProjectInfo> {
  const all = await listAllProjects();
  const match = all.find((p) => p.id === id);
  if (!match) throw new Error(`Project "${id}" not found`);
  return match;
}

/** List both archived and active projects. */
async function listAllProjects(): Promise<ProjectInfo[]> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", {
    type: "project",
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources.map(toProjectInfo);
}

export async function archiveProject(id: string): Promise<void> {
  const project = await getProject(id);
  await updateProject(project, {
    archivedAt: new Date().toISOString(),
  });
}

export async function restoreProject(id: string): Promise<void> {
  const project = await getProject(id);
  await updateProject(project, { archivedAt: null });
}

export async function reorderProjects(
  order: [string, number][],
): Promise<void> {
  const all = await listAllProjects();
  for (const [id, orderValue] of order) {
    const existing = all.find((p) => p.id === id);
    if (!existing) continue;
    await updateProject(existing, { order: orderValue });
  }
}

export async function listArchivedProjects(): Promise<ProjectInfo[]> {
  const all = await listAllProjects();
  return all.filter((p) => p.archivedAt !== null);
}
