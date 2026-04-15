import { invoke } from "@tauri-apps/api/core";

export interface AcpProvider {
  id: string;
  label: string;
}

export interface AcpSendMessageOptions {
  systemPrompt?: string;
  workingDir?: string;
  personaId?: string;
  personaName?: string;
  /** Image attachments as [base64Data, mimeType] pairs. */
  images?: [string, string][];
}

export interface AcpPrepareSessionOptions {
  workingDir?: string;
  personaId?: string;
}

/** Discover ACP providers installed on the system. */
export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  return invoke("discover_acp_providers");
}

/** Send a message to an ACP agent. Response streams via Tauri events. */
export async function acpSendMessage(
  sessionId: string,
  providerId: string,
  prompt: string,
  options: AcpSendMessageOptions = {},
): Promise<void> {
  const { systemPrompt, workingDir, personaId, personaName, images } = options;
  return invoke("acp_send_message", {
    sessionId,
    providerId,
    prompt,
    systemPrompt: systemPrompt ?? null,
    workingDir: workingDir ?? null,
    personaId: personaId ?? null,
    personaName: personaName ?? null,
    images: images ?? [],
  });
}

/** Prepare or warm an ACP session ahead of the first prompt. */
export async function acpPrepareSession(
  sessionId: string,
  providerId: string,
  options: AcpPrepareSessionOptions = {},
): Promise<void> {
  const { workingDir, personaId } = options;
  return invoke("acp_prepare_session", {
    sessionId,
    providerId,
    workingDir: workingDir ?? null,
    personaId: personaId ?? null,
  });
}

export async function acpSetModel(
  sessionId: string,
  modelId: string,
): Promise<void> {
  return invoke("acp_set_model", {
    sessionId,
    modelId,
  });
}

/** Session info returned by the goose binary's list_sessions. */
export interface AcpSessionInfo {
  sessionId: string;
  title: string | null;
  updatedAt: string | null;
  messageCount: number;
}

export interface AcpSessionSearchResult {
  sessionId: string;
  snippet: string;
  messageId: string;
  messageRole?: "user" | "assistant" | "system";
  matchCount: number;
}

/** List all sessions known to the goose binary. */
export async function acpListSessions(): Promise<AcpSessionInfo[]> {
  return invoke("acp_list_sessions");
}

export async function acpSearchSessions(
  query: string,
  sessionIds: string[],
): Promise<AcpSessionSearchResult[]> {
  return invoke("acp_search_sessions", { query, sessionIds });
}

/**
 * Load an existing session from the goose binary.
 *
 * This triggers message replay via SessionNotification events that the
 * frontend's useAcpStream hook picks up automatically.
 */
export async function acpLoadSession(
  sessionId: string,
  gooseSessionId: string,
  workingDir?: string,
): Promise<void> {
  return invoke("acp_load_session", {
    sessionId,
    gooseSessionId,
    workingDir: workingDir ?? null,
  });
}

/** Export a session as JSON via the goose binary. */
export async function acpExportSession(sessionId: string): Promise<string> {
  return invoke("acp_export_session", { sessionId });
}

/** Import a session from JSON via the goose binary. Returns new session metadata. */
export async function acpImportSession(json: string): Promise<AcpSessionInfo> {
  return invoke("acp_import_session", { json });
}

/** Duplicate (fork) a session via the goose binary. Returns new session metadata. */
export async function acpDuplicateSession(
  sessionId: string,
): Promise<AcpSessionInfo> {
  return invoke("acp_duplicate_session", { sessionId });
}

/** Cancel an in-progress ACP session so the backend stops streaming. */
export async function acpCancelSession(
  sessionId: string,
  personaId?: string,
): Promise<boolean> {
  return invoke("acp_cancel_session", {
    sessionId,
    personaId: personaId ?? null,
  });
}
