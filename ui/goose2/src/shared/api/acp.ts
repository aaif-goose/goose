import { invoke } from "@tauri-apps/api/core";
import { USE_DIRECT_ACP } from "./acpFeatureFlag";
import * as directAcp from "./acpApi";
import * as sessionTracker from "./acpSessionTracker";
import {
  setActiveMessageId,
  clearActiveMessageId,
} from "./acpNotificationHandler";
import { searchSessionsViaExports } from "./sessionSearch";

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
  if (USE_DIRECT_ACP) {
    return directAcp.listProviders();
  }
  return invoke("discover_acp_providers");
}

/** Send a message to an ACP agent. Response streams via Tauri events. */
export async function acpSendMessage(
  sessionId: string,
  providerId: string,
  prompt: string,
  options: AcpSendMessageOptions = {},
): Promise<void> {
  if (USE_DIRECT_ACP) {
    const { systemPrompt, personaId, images } = options;

    const gooseSessionId = sessionTracker.getGooseSessionId(
      sessionId,
      personaId,
    );
    if (!gooseSessionId) {
      throw new Error("Session not prepared. Call acpPrepareSession first.");
    }

    const hasSystem = systemPrompt && systemPrompt.trim().length > 0;
    const effectivePrompt = hasSystem
      ? `<persona-instructions>\n${systemPrompt}\n</persona-instructions>\n\n<user-message>\n${prompt}\n</user-message>`
      : prompt;

    const content: import("@agentclientprotocol/sdk").ContentBlock[] = [
      { type: "text", text: effectivePrompt },
    ];
    if (images) {
      for (const [data, mimeType] of images) {
        content.push({ type: "image", data, mimeType } as any);
      }
    }

    const messageId = crypto.randomUUID();
    setActiveMessageId(gooseSessionId, messageId);

    await directAcp.prompt(gooseSessionId, content);

    clearActiveMessageId(gooseSessionId);
    return;
  }
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
  if (USE_DIRECT_ACP) {
    const workingDir = options.workingDir ?? "~/.goose/artifacts";
    await sessionTracker.prepareSession(
      sessionId,
      providerId,
      workingDir,
      options.personaId,
    );
    return;
  }
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
  if (USE_DIRECT_ACP) {
    const gooseSessionId = sessionTracker.getGooseSessionId(sessionId);
    return directAcp.setModel(gooseSessionId ?? sessionId, modelId);
  }
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
  if (USE_DIRECT_ACP) {
    return directAcp.listSessions();
  }
  return invoke("acp_list_sessions");
}

export async function acpSearchSessions(
  query: string,
  sessionIds: string[],
): Promise<AcpSessionSearchResult[]> {
  if (USE_DIRECT_ACP) {
    return searchSessionsViaExports(query, sessionIds);
  }
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
  if (USE_DIRECT_ACP) {
    const effectiveWorkingDir = workingDir ?? "~/.goose/artifacts";
    await directAcp.loadSession(gooseSessionId, effectiveWorkingDir);
    sessionTracker.registerSession(
      sessionId,
      gooseSessionId,
      "goose",
      effectiveWorkingDir,
    );
    return;
  }
  return invoke("acp_load_session", {
    sessionId,
    gooseSessionId,
    workingDir: workingDir ?? null,
  });
}

/** Export a session as JSON via the goose binary. */
export async function acpExportSession(sessionId: string): Promise<string> {
  if (USE_DIRECT_ACP) {
    return directAcp.exportSession(sessionId);
  }
  return invoke("acp_export_session", { sessionId });
}

/** Import a session from JSON via the goose binary. Returns new session metadata. */
export async function acpImportSession(json: string): Promise<AcpSessionInfo> {
  if (USE_DIRECT_ACP) {
    return directAcp.importSession(json);
  }
  return invoke("acp_import_session", { json });
}

/** Duplicate (fork) a session via the goose binary. Returns new session metadata. */
export async function acpDuplicateSession(
  sessionId: string,
): Promise<AcpSessionInfo> {
  if (USE_DIRECT_ACP) {
    const gooseSessionId =
      sessionTracker.getGooseSessionId(sessionId) ?? sessionId;
    return directAcp.forkSession(gooseSessionId);
  }
  return invoke("acp_duplicate_session", { sessionId });
}

/** Cancel an in-progress ACP session so the backend stops streaming. */
export async function acpCancelSession(
  sessionId: string,
  personaId?: string,
): Promise<boolean> {
  if (USE_DIRECT_ACP) {
    const gooseSessionId = sessionTracker.getGooseSessionId(
      sessionId,
      personaId,
    );
    await directAcp.cancelSession(gooseSessionId ?? sessionId);
    return true;
  }
  return invoke("acp_cancel_session", {
    sessionId,
    personaId: personaId ?? null,
  });
}
