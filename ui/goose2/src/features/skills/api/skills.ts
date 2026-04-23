import { getClient } from "@/shared/api/acpConnection";

export interface SkillInfo {
  name: string;
  description: string;
  instructions: string;
  path: string;
}

// Shape returned by _goose/sources/*. Narrowed to skill-type sources here.
interface SourceEntry {
  type: "skill";
  name: string;
  description: string;
  content: string;
  directory: string;
  global: boolean;
}

function toSkillInfo(source: SourceEntry): SkillInfo {
  return {
    name: source.name,
    description: source.description,
    instructions: source.content,
    path: source.directory,
  };
}

export async function createSkill(
  name: string,
  description: string,
  instructions: string,
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/sources/create", {
    type: "skill",
    name,
    description,
    content: instructions,
    global: true,
  });
}

export async function listSkills(): Promise<SkillInfo[]> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/list", { type: "skill" });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources.map(toSkillInfo);
}

export async function deleteSkill(name: string): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/sources/delete", {
    type: "skill",
    name,
    global: true,
  });
}

export async function updateSkill(
  name: string,
  description: string,
  instructions: string,
): Promise<SkillInfo> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/update", {
    type: "skill",
    name,
    description,
    content: instructions,
    global: true,
  });
  return toSkillInfo(raw.source as SourceEntry);
}

export async function exportSkill(
  name: string,
): Promise<{ json: string; filename: string }> {
  const client = await getClient();
  const raw = await client.extMethod("_goose/sources/export", {
    type: "skill",
    name,
    global: true,
  });
  return { json: raw.json as string, filename: raw.filename as string };
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
  const raw = await client.extMethod("_goose/sources/import", {
    data,
    global: true,
  });
  const sources = (raw.sources ?? []) as SourceEntry[];
  return sources.map(toSkillInfo);
}
