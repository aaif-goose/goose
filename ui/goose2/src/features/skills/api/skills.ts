import type { SourceEntry } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";

const SKILL_SOURCE_TYPE = "skill" as const;

export interface SkillInfo {
  name: string;
  description: string;
  instructions: string;
  path: string;
  fileLocation: string;
}

type SkillSourceEntry = SourceEntry & { type: typeof SKILL_SOURCE_TYPE };

function isSkillSource(source: SourceEntry): source is SkillSourceEntry {
  return source.type === SKILL_SOURCE_TYPE;
}

function getSkillFileLocation(directory: string): string {
  const separator = directory.includes("\\") ? "\\" : "/";
  return directory.endsWith(separator)
    ? `${directory}SKILL.md`
    : `${directory}${separator}SKILL.md`;
}

function toSkillInfo(source: SkillSourceEntry): SkillInfo {
  return {
    name: source.name,
    description: source.description,
    instructions: source.content,
    path: source.directory,
    fileLocation: getSkillFileLocation(source.directory),
  };
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

export async function listSkills(): Promise<SkillInfo[]> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesList({
    type: SKILL_SOURCE_TYPE,
  });
  return response.sources.filter(isSkillSource).map(toSkillInfo);
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
