import { invoke } from "@tauri-apps/api/core";
import type { SourceEntry } from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import type {
  Persona,
  CreatePersonaRequest,
  UpdatePersonaRequest,
  Avatar,
} from "@/shared/types/agents";

const AGENT_SOURCE_TYPE = "agent" as const;
const AGENT_DESCRIPTION = "Agent";

type AgentSourceProperties = {
  provider?: string;
  model?: string;
  avatar?: string;
};

type AgentSourceEntry = SourceEntry & {
  type: typeof AGENT_SOURCE_TYPE;
  properties: AgentSourceProperties;
};

function isAgentSource(source: SourceEntry): source is AgentSourceEntry {
  return source.type === AGENT_SOURCE_TYPE;
}

function avatarToProperty(
  avatar: Avatar | null | undefined,
): string | undefined {
  if (!avatar) return undefined;
  return avatar.value;
}

function propertyToAvatar(value: string | undefined): Avatar | null {
  if (!value) return null;
  return { type: "url", value };
}

function personaProperties(
  request: CreatePersonaRequest | UpdatePersonaRequest,
): AgentSourceProperties | undefined {
  const properties: AgentSourceProperties = {};
  if (request.provider) properties.provider = request.provider;
  if (request.model) properties.model = request.model;
  const avatar = avatarToProperty(request.avatar);
  if (avatar) properties.avatar = avatar;
  return properties;
}

function toPersona(source: AgentSourceEntry): Persona {
  const writable = source.writable !== false;
  return {
    id: source.path,
    displayName: source.name,
    avatar: propertyToAvatar(source.properties?.avatar),
    systemPrompt: source.content,
    provider: source.properties?.provider,
    model: source.properties?.model,
    isBuiltin: !writable,
    isFromDisk: writable,
    writable,
    createdAt: "",
    updatedAt: "",
  };
}

async function listAgentSources(): Promise<AgentSourceEntry[]> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesList({
    type: AGENT_SOURCE_TYPE,
  });
  return response.sources.filter(isAgentSource);
}

async function getAgentSource(id: string): Promise<AgentSourceEntry> {
  const source = (await listAgentSources()).find(
    (source) => source.path === id,
  );
  if (!source) {
    throw new Error(`Agent '${id}' not found`);
  }
  return source;
}

export async function listPersonas(): Promise<Persona[]> {
  return (await listAgentSources()).map(toPersona);
}

export async function createPersona(
  request: CreatePersonaRequest,
): Promise<Persona> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesCreate({
    type: AGENT_SOURCE_TYPE,
    name: request.displayName,
    description: AGENT_DESCRIPTION,
    content: request.systemPrompt,
    properties: personaProperties(request),
    global: true,
  });

  if (!isAgentSource(response.source)) {
    throw new Error(`Unexpected source type returned: ${response.source.type}`);
  }

  return toPersona(response.source);
}

export async function updatePersona(
  id: string,
  request: UpdatePersonaRequest,
): Promise<Persona> {
  const existing = await getAgentSource(id);
  const client = await getClient();
  const merged: CreatePersonaRequest = {
    displayName: request.displayName ?? existing.name,
    avatar:
      request.avatar === undefined
        ? propertyToAvatar(existing.properties?.avatar)
        : request.avatar,
    systemPrompt: request.systemPrompt ?? existing.content,
    provider: request.provider ?? existing.properties?.provider,
    model: request.model ?? existing.properties?.model,
  };
  const response = await client.goose.GooseSourcesUpdate({
    type: AGENT_SOURCE_TYPE,
    path: id,
    name: merged.displayName,
    description: existing.description || AGENT_DESCRIPTION,
    content: merged.systemPrompt,
    properties: personaProperties(merged),
  });

  if (!isAgentSource(response.source)) {
    throw new Error(`Unexpected source type returned: ${response.source.type}`);
  }

  return toPersona(response.source);
}

export async function deletePersona(id: string): Promise<void> {
  const client = await getClient();
  await client.goose.GooseSourcesDelete({
    type: AGENT_SOURCE_TYPE,
    path: id,
  });
}

export async function refreshPersonas(): Promise<Persona[]> {
  return listPersonas();
}

export interface ExportResult {
  json: string;
  suggestedFilename: string;
}

export async function exportPersona(id: string): Promise<ExportResult> {
  const client = await getClient();
  const response = await client.goose.GooseSourcesExport({
    type: AGENT_SOURCE_TYPE,
    path: id,
  });
  return { json: response.json, suggestedFilename: response.filename };
}

export async function importPersonas(
  fileBytes: number[],
  fileName: string,
): Promise<Persona[]> {
  if (
    !fileName.endsWith(".agent.json") &&
    !fileName.endsWith(".persona.json") &&
    !fileName.endsWith(".json")
  ) {
    throw new Error(
      "File must have a .agent.json, .persona.json, or .json extension",
    );
  }

  const raw = new TextDecoder().decode(new Uint8Array(fileBytes));
  const parsed = JSON.parse(raw) as Record<string, unknown>;
  const data =
    parsed.type === AGENT_SOURCE_TYPE
      ? raw
      : JSON.stringify({
          version: parsed.version ?? 1,
          type: AGENT_SOURCE_TYPE,
          name: parsed.displayName ?? parsed.name,
          description: AGENT_DESCRIPTION,
          content:
            parsed.systemPrompt ?? parsed.content ?? parsed.instructions ?? "",
          properties: {
            provider: parsed.provider,
            model: parsed.model,
            avatar:
              typeof parsed.avatar === "string"
                ? parsed.avatar
                : typeof parsed.avatar === "object" &&
                    parsed.avatar !== null &&
                    "value" in parsed.avatar
                  ? (parsed.avatar as { value?: unknown }).value
                  : undefined,
          },
        });

  const client = await getClient();
  const response = await client.goose.GooseSourcesImport({
    data,
    global: true,
  });

  return response.sources.filter(isAgentSource).map(toPersona);
}

export interface ImportFileReadResult {
  fileBytes: number[];
  fileName: string;
}

export async function readImportPersonaFile(
  sourcePath: string,
): Promise<ImportFileReadResult> {
  return invoke("read_import_persona_file", { sourcePath });
}
