import type { SourceEntry } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import {
  basename,
  deriveProjectRoot,
  getSkillFileLocation,
} from "../lib/skillsPath";

const SKILL_SOURCE_TYPE = "skill" as const;

export interface SkillProjectLink {
  id: string;
  name: string;
  workingDir: string;
}

export type SkillSourceKind = "global" | "project";

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

type SkillSourceEntry = SourceEntry & { type: typeof SKILL_SOURCE_TYPE };

function isSkillSource(source: SourceEntry): source is SkillSourceEntry {
  return source.type === SKILL_SOURCE_TYPE;
}

function toSkillInfo(source: SkillSourceEntry): SkillInfo {
  const sourceKind: SkillSourceKind = source.global ? "global" : "project";
  const projectRoot = source.global
    ? null
    : deriveProjectRoot(source.directory);
  const projectName = projectRoot ? basename(projectRoot) : "";

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
    id: `${sourceKind}:${source.directory}`,
    name: source.name,
    description: source.description,
    instructions: source.content,
    path: source.directory,
    fileLocation: getSkillFileLocation(source.directory),
    sourceKind,
    sourceLabel:
      sourceKind === "global" ? "Personal" : projectName || "Project",
    projectLinks,
  };
}

function uniqueProjectDirs(projectDirs: string[]) {
  return [...new Set(projectDirs.map((dir) => dir.trim()).filter(Boolean))];
}

export async function createSkill(
  name: string,
  description: string,
  instructions: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseSourcesCreate({
    type: SKILL_SOURCE_TYPE,
    name,
    description,
    content: instructions,
    global: true,
  });
}

export async function listSkills(
  projectDirs: string[] = [],
): Promise<SkillInfo[]> {
  const client = await getClient();
  const fetchSources = (projectDir?: string) =>
    client.goose.GooseSourcesList({
      type: SKILL_SOURCE_TYPE,
      ...(projectDir ? { projectDir } : {}),
    });

  const globalResponse = await fetchSources();
  const projectResponses = await Promise.allSettled(
    uniqueProjectDirs(projectDirs).map((projectDir) =>
      fetchSources(projectDir),
    ),
  );
  const responses = [
    { response: globalResponse, projectResponse: false },
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

      const key = `${source.global ? "global" : "project"}:${source.directory}`;
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

  if (!isSkillSource(response.source)) {
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

  return response.sources.filter(isSkillSource).map(toSkillInfo);
}
