import { invoke } from "@tauri-apps/api/core";
import type {
  Persona,
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";

export async function listPersonas(): Promise<Persona[]> {
  return invoke("list_personas");
}

export async function createPersona(
  request: CreatePersonaRequest,
): Promise<Persona> {
  return invoke("create_persona", { request });
}

export async function updatePersona(
  id: string,
  request: UpdatePersonaRequest,
): Promise<Persona> {
  return invoke("update_persona", { id, request });
}

export async function deletePersona(id: string): Promise<void> {
  return invoke("delete_persona", { id });
}

export async function refreshPersonas(): Promise<Persona[]> {
  return invoke("refresh_personas");
}

export interface ExportResult {
  json: string;
  suggestedFilename: string;
}

export async function exportPersona(id: string): Promise<ExportResult> {
  return invoke("export_persona", { id });
}

export async function importPersonas(
  fileBytes: number[],
  fileName: string,
): Promise<Persona[]> {
  return invoke("import_personas", { fileBytes, fileName });
}

export async function savePersonaAvatar(
  personaId: string,
  sourcePath: string,
): Promise<string> {
  return invoke("save_persona_avatar", { personaId, sourcePath });
}

export async function savePersonaAvatarBytes(
  personaId: string,
  bytes: number[],
  extension: string,
): Promise<string> {
  return invoke("save_persona_avatar_bytes", { personaId, bytes, extension });
}

export async function getAvatarsDir(): Promise<string> {
  return invoke("get_avatars_dir");
}
