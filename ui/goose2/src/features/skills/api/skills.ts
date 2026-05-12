import type { SourceEntry } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import {
  basename,
  deriveProjectRoot,
  getSkillFileLocation,
} from "../lib/skillsPath";

const SKILL_SOURCE_TYPE = "skill" as const;
const BUILTIN_SKILL_SOURCE_TYPE = "builtinSkill" as const;

export interface SkillProjectLink {
  id: string;
  name: string;
  workingDir: string;
}

export type SkillSourceKind = "global" | "project" | "builtin";

export interface SkillInfo {
  id: string;
  name: string;
  description: string;
  instructions: string;
  path: string;
  fileLocation: string;
  sourceKind: SkillSourceKind;
  sourceLabel: string;
  projectLinks: SkillProjectLink[];
}

export type EditingSkill = Pick<
  SkillInfo,
  "name" | "description" | "instructions" | "path" | "fileLocation"
>;

type FilesystemSkillSourceEntry = SourceEntry & {
  type: typeof SKILL_SOURCE_TYPE;
};
type BuiltinSkillSourceEntry = SourceEntry & {
  type: typeof BUILTIN_SKILL_SOURCE_TYPE;
};
type SkillSourceEntry = FilesystemSkillSourceEntry | BuiltinSkillSourceEntry;

function isFilesystemSkillSource(
  source: SourceEntry,
): source is FilesystemSkillSourceEntry {
  return source.type === SKILL_SOURCE_TYPE;
}

function isSkillSource(source: SourceEntry): source is SkillSourceEntry {
  return (
    source.type === SKILL_SOURCE_TYPE ||
    source.type === BUILTIN_SKILL_SOURCE_TYPE
  );
}

function toSkillInfo(source: SkillSourceEntry): SkillInfo {
  if (source.type === BUILTIN_SKILL_SOURCE_TYPE) {
    return {
      id: `builtin:${source.name}`,
      name: source.name,
      description: source.description,
      instructions: source.content,
      path: source.path,
      fileLocation: source.path,
      sourceKind: "builtin",
      sourceLabel: "Built in",
      projectLinks: [],
    };
  }

  const sourceKind: SkillSourceKind = source.global ? "global" : "project";
  const props = (source.properties ?? {}) as Record<string, unknown>;

  // Backend tags project-scoped skills with these when listing via
  // include_project_sources. Prefer them over path-derived values so badges
  // show the user-visible project title.
  const taggedProjectDir =
    typeof props.projectDir === "string" ? props.projectDir : null;
  const taggedProjectName =
    typeof props.projectName === "string" ? props.projectName : null;

  const projectRoot = source.global
    ? null
    : (taggedProjectDir ?? deriveProjectRoot(source.path));
  const projectName =
    taggedProjectName ?? (projectRoot ? basename(projectRoot) : "");

  const projectLinks: SkillProjectLink[] = projectRoot
    ? [
        {
          id: projectRoot,
          name: projectName || projectRoot,
          workingDir: projectRoot,
        },
      ]
    : [];

  return {
    id: `${sourceKind}:${source.path}`,
    name: source.name,
    description: source.description,
    instructions: source.content,
    path: source.path,
    fileLocation: getSkillFileLocation(source.path),
    sourceKind,
    sourceLabel:
      sourceKind === "global" ? "Personal" : projectName || "Project",
    projectLinks,
  };
}

function uniqueProjectDirs(projectDirs: string[]) {
  return [...new Set(projectDirs.map((dir) => dir.trim()).filter(Boolean))];
}

export interface CreateSkillOptions {
  /** Project source ID (kebab slug). When set, the skill is created under
   *  that project's first working directory. */
  projectId?: string;
}

export async function createSkill(
  name: string,
  description: string,
  instructions: string,
  options: CreateSkillOptions = {},
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseSourcesCreate({
    type: SKILL_SOURCE_TYPE,
    name,
    description,
    content: instructions,
    global: !options.projectId,
    ...(options.projectId ? { projectId: options.projectId } : {}),
  });
}

export async function listSkills(
  projectDirs: string[] = [],
): Promise<SkillInfo[]> {
  const client = await getClient();
  const fetchSources = (
    type: typeof SKILL_SOURCE_TYPE | typeof BUILTIN_SKILL_SOURCE_TYPE,
    projectDir?: string,
  ) =>
    client.goose.GooseSourcesList({
      type,
      ...(projectDir ? { projectDir } : {}),
    });

  const [globalResponse, builtinResponse] = await Promise.all([
    fetchSources(SKILL_SOURCE_TYPE),
    fetchSources(BUILTIN_SKILL_SOURCE_TYPE).catch(() => null),
  ]);
  const projectResponses = await Promise.allSettled(
    uniqueProjectDirs(projectDirs).map((projectDir) =>
      fetchSources(SKILL_SOURCE_TYPE, projectDir),
    ),
  );
  const responses = [
    { response: globalResponse, projectResponse: false },
    ...(builtinResponse
      ? [{ response: builtinResponse, projectResponse: false }]
      : []),
    ...projectResponses.flatMap((result) =>
      result.status === "fulfilled"
        ? [{ response: result.value, projectResponse: true }]
        : [],
    ),
  ];
  const seen = new Set<string>();
  const skills: SkillInfo[] = [];

  responses.forEach(({ response, projectResponse }) => {
    for (const source of response.sources) {
      if (!isSkillSource(source) || (projectResponse && source.global)) {
        continue;
      }

      const key =
        source.type === BUILTIN_SKILL_SOURCE_TYPE
          ? `builtin:${source.name}`
          : `${source.global ? "global" : "project"}:${source.path}`;
      if (seen.has(key)) {
        continue;
      }

      seen.add(key);
      skills.push(toSkillInfo(source));
    }
  });

  return skills;
}

export async function deleteSkill(path: string): Promise<void> {
  const client = await getClient();
  await client.goose.GooseSourcesDelete({
    type: SKILL_SOURCE_TYPE,
    path,
  });
}

export async function updateSkill(
  path: string,
  name: string,
  description: string,
  instructions: string,
): Promise<SkillInfo> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesUpdate({
    type: SKILL_SOURCE_TYPE,
    path,
    name,
    description,
    content: instructions,
  });

  if (!isFilesystemSkillSource(response.source)) {
    throw new Error(`Unexpected source type returned: ${response.source.type}`);
  }

  return toSkillInfo(response.source);
}

export async function exportSkill(
  path: string,
): Promise<{ json: string; filename: string }> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesExport({
    type: SKILL_SOURCE_TYPE,
    path,
  });
  return { json: response.json, filename: response.filename };
}

export async function importSkills(
  fileBytes: number[],
  fileName: string,
): Promise<SkillInfo[]> {
  if (!fileName.endsWith(".skill.json") && !fileName.endsWith(".json")) {
    throw new Error("File must have a .skill.json or .json extension");
  }

  const data = new TextDecoder().decode(new Uint8Array(fileBytes));
  const client = await getClient();
  const response = await client.goose.GooseSourcesImport({
    data,
    global: true,
  });

  return response.sources.filter(isFilesystemSkillSource).map(toSkillInfo);
}
