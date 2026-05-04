import type {
  SessionNotification,
  SessionUpdate,
} from "@agentclientprotocol/sdk";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import {
  getBufferedMessage,
  findLatestUnpairedToolRequest,
} from "@/features/chat/hooks/replayBuffer";
import type {
  ToolCallLocation,
  ToolCallStatus,
  ToolKind,
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";
import type { AcpNotificationHandler } from "./acpConnection";
import { handleReplayUserMessageChunk } from "./acpSkillReplayChips";
import {
  attachMcpAppPayload,
  extractToolStructuredContent,
  extractToolResultText,
  findReplayMessageWithToolCall,
} from "./acpToolCallContent";
import {
  clearReplayAssistantMessage,
  clearReplayAssistantTracking,
  ensureReplayAssistantMessage,
  getTrackedReplayAssistantMessageId,
} from "./acpReplayAssistant";
import { getReplayCreated, getReplayMessageId } from "./acpReplayMetadata";
import {
  getLocalSessionId,
  subscribeToSessionRegistration,
} from "./acpSessionTracker";
import { getToolCallIdentity } from "./acpToolCallIdentity";
import { perfLog } from "@/shared/lib/perfLog";

// Pre-set message ID for the next live stream per goose session
const presetMessageIds = new Map<string, string>();

// Per-session perf counters for replay/live streaming.
interface ReplayPerf {
  firstAt: number;
  lastAt: number;
  count: number;
}
const replayPerf = new Map<string, ReplayPerf>();
interface LivePerf {
  sendStartedAt: number;
  firstChunkAt: number | null;
  chunkCount: number;
}
const livePerf = new Map<string, LivePerf>();
const pendingUsageUpdates = new Map<
  string,
  { accumulatedTotal: number; contextLimit: number }
>();

const toolCallStatusFromUpdate = (status: string): ToolCallStatus =>
  status === "failed" ? "error" : "completed";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function rawInputToArguments(rawInput: unknown): Record<string, unknown> {
  return isRecord(rawInput) ? rawInput : {};
}

function toolKindFromUpdate(update: SessionUpdate): ToolKind | undefined {
  const record: Record<string, unknown> = update;
  const value = record.kind;
  return typeof value === "string" ? (value as ToolKind) : undefined;
}

function locationsFromUpdate(
  update: SessionUpdate,
): ToolCallLocation[] | undefined {
  const record: Record<string, unknown> = update;
  const value = record.locations;
  if (!Array.isArray(value)) return undefined;

  return value
    .filter(
      (location): location is { path: string; line?: number | null } =>
        isRecord(location) && typeof location.path === "string",
    )
    .map((location) => ({
      path: location.path,
      ...(typeof location.line === "number" || location.line === null
        ? { line: location.line }
        : {}),
    }));
}

function toolCallUpdatePatch(
  update: SessionUpdate,
): Partial<ToolRequestContent> {
  const toolKind = toolKindFromUpdate(update);
  const locations = locationsFromUpdate(update);

  return {
    ...(toolKind ? { toolKind } : {}),
    ...(locations ? { locations } : {}),
  };
}

subscribeToSessionRegistration((localSessionId, gooseSessionId) => {
  const pendingUsage = pendingUsageUpdates.get(gooseSessionId);
  if (!pendingUsage) {
    return;
  }

  useChatStore.getState().updateTokenState(localSessionId, pendingUsage);
  pendingUsageUpdates.delete(gooseSessionId);
});

export function setActiveMessageId(
  gooseSessionId: string,
  messageId: string,
): void {
  presetMessageIds.set(gooseSessionId, messageId);
  livePerf.set(gooseSessionId, {
    sendStartedAt: performance.now(),
    firstChunkAt: null,
    chunkCount: 0,
  });
}

export function clearActiveMessageId(gooseSessionId: string): void {
  presetMessageIds.delete(gooseSessionId);
  const perf = livePerf.get(gooseSessionId);
  if (perf) {
    const sid = gooseSessionId.slice(0, 8);
    const total = performance.now() - perf.sendStartedAt;
    const ttft =
      perf.firstChunkAt !== null
        ? (perf.firstChunkAt - perf.sendStartedAt).toFixed(1)
        : "n/a";
    perfLog(
      `[perf:stream] ${sid} stream ended — ttft=${ttft}ms total=${total.toFixed(1)}ms chunks=${perf.chunkCount}`,
    );
    livePerf.delete(gooseSessionId);
  }
}

export async function handleSessionNotification(
  notification: SessionNotification,
): Promise<void> {
  const gooseSessionId = notification.sessionId;
  const localSessionId = getLocalSessionId(gooseSessionId);
  const sessionId = localSessionId ?? gooseSessionId;
  const { update } = notification;
  const isReplay = useChatStore.getState().loadingSessionIds.has(sessionId);

  if (isReplay) {
    const sid = sessionId.slice(0, 8);
    let perf = replayPerf.get(sessionId);
    const now = performance.now();
    if (!perf) {
      perf = { firstAt: now, lastAt: now, count: 0 };
      replayPerf.set(sessionId, perf);
      perfLog(`[perf:replay] ${sid} first notification received`);
    }
    perf.lastAt = now;
    perf.count += 1;
    handleReplay(sessionId, gooseSessionId, localSessionId, update);
  } else {
    const perf = livePerf.get(gooseSessionId);
    if (perf && update.sessionUpdate === "agent_message_chunk") {
      perf.chunkCount += 1;
      if (perf.firstChunkAt === null) {
        perf.firstChunkAt = performance.now();
        const sid = gooseSessionId.slice(0, 8);
        perfLog(
          `[perf:stream] ${sid} first agent_message_chunk at ttft=${(perf.firstChunkAt - perf.sendStartedAt).toFixed(1)}ms`,
        );
      }
    }
    handleLive(sessionId, gooseSessionId, localSessionId, update);
  }
}

export function getReplayPerf(
  sessionId: string,
): { count: number; spanMs: number } | null {
  const perf = replayPerf.get(sessionId);
  if (!perf) return null;
  return { count: perf.count, spanMs: perf.lastAt - perf.firstAt };
}

export function clearReplayPerf(sessionId: string): void {
  replayPerf.delete(sessionId);
}

function getChunkMessageId(update: SessionUpdate): string | null {
  return "messageId" in update && typeof update.messageId === "string"
    ? update.messageId
    : null;
}

function handleReplay(
  sessionId: string,
  gooseSessionId: string,
  localSessionId: string | null,
  update: SessionUpdate,
): void {
  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const msg = ensureReplayAssistantMessage(
        sessionId,
        getReplayMessageId(update),
        getReplayCreated(update),
      );
      if (msg && update.content.type === "text" && "text" in update.content) {
        const last = msg.content[msg.content.length - 1];
        if (last?.type === "text") {
          (last as { type: "text"; text: string }).text += update.content.text;
        } else {
          msg.content.push({ type: "text", text: update.content.text });
        }
      }
      break;
    }

    case "user_message_chunk": {
      clearReplayAssistantMessage(sessionId);
      if (update.content.type !== "text" || !("text" in update.content)) break;
      const messageId = getReplayMessageId(update) ?? crypto.randomUUID();
      handleReplayUserMessageChunk(
        sessionId,
        messageId,
        update.content,
        getReplayCreated(update),
      );
      break;
    }

    case "tool_call": {
      const created = getReplayCreated(update);
      const identity = getToolCallIdentity(update);
      const msg = ensureReplayAssistantMessage(
        sessionId,
        getReplayMessageId(update),
        created,
      );
      msg.content.push({
        type: "toolRequest",
        id: update.toolCallId,
        name: update.title,
        ...identity,
        arguments: rawInputToArguments(update.rawInput),
        status: "executing",
        ...toolCallUpdatePatch(update),
        startedAt: created ?? Date.now(),
      });
      break;
    }

    case "tool_call_update": {
      const created = getReplayCreated(update);
      const replayMessageId = getReplayMessageId(update);
      const identity = getToolCallIdentity(update);
      const trackedMessageId = getTrackedReplayAssistantMessageId(sessionId);
      const replayMsg = replayMessageId
        ? getBufferedMessage(sessionId, replayMessageId)
        : undefined;
      const trackedMsg =
        trackedMessageId && trackedMessageId !== replayMessageId
          ? getBufferedMessage(sessionId, trackedMessageId)
          : undefined;
      const existingMsg = findReplayMessageWithToolCall(
        sessionId,
        update.toolCallId,
      );
      const msg = existingMsg ?? replayMsg ?? trackedMsg;
      if (msg) {
        if (created !== undefined && !existingMsg && msg === replayMsg) {
          msg.created = created;
        }
        const patch = toolCallUpdatePatch(update);
        if (
          update.title ||
          Object.keys(identity).length > 0 ||
          Object.keys(patch).length > 0
        ) {
          const tc = msg.content.find(
            (c) => c.type === "toolRequest" && c.id === update.toolCallId,
          );
          if (tc && tc.type === "toolRequest") {
            Object.assign(tc as ToolRequestContent, {
              ...(update.title ? { name: update.title } : {}),
              ...identity,
              ...patch,
            });
          }
        }
        if (update.status === "completed" || update.status === "failed") {
          const toolCallStatus = toolCallStatusFromUpdate(update.status);
          const tc = msg.content.find(
            (c) => c.type === "toolRequest" && c.id === update.toolCallId,
          );
          if (tc && tc.type === "toolRequest") {
            const idx = msg.content.indexOf(tc);
            if (idx >= 0) {
              msg.content[idx] = {
                ...tc,
                ...identity,
                ...toolCallUpdatePatch(update),
                status: toolCallStatus,
              } as ToolRequestContent;
            }
          }
          const resultText = extractToolResultText(update);
          msg.content.push({
            type: "toolResponse",
            id: update.toolCallId,
            name: (tc as ToolRequestContent)?.name ?? "",
            result: resultText,
            structuredContent: extractToolStructuredContent(update),
            isError: update.status === "failed",
          });
          if (update.status === "completed") {
            attachMcpAppPayload(
              sessionId,
              update.toolCallId,
              (tc as ToolRequestContent)?.name ?? update.title ?? "",
              update,
              true,
              {
                gooseSessionId,
                replayMessageId,
              },
            );
          }
        }
      }
      break;
    }

    case "session_info_update":
    case "config_option_update":
    case "usage_update":
      handleShared(sessionId, gooseSessionId, localSessionId, update);
      break;

    default:
      break;
  }
}

function handleLive(
  sessionId: string,
  gooseSessionId: string,
  localSessionId: string | null,
  update: SessionUpdate,
): void {
  const store = useChatStore.getState();

  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const messageId = ensureLiveAssistantMessage(
        sessionId,
        gooseSessionId,
        getChunkMessageId(update) ?? undefined,
      );

      if (update.content.type === "text" && "text" in update.content) {
        store.setStreamingMessageId(sessionId, messageId);
        store.updateStreamingText(sessionId, update.content.text);
      }
      break;
    }

    case "tool_call": {
      const messageId = ensureLiveAssistantMessage(sessionId, gooseSessionId);
      const identity = getToolCallIdentity(update);

      const toolRequest: ToolRequestContent = {
        type: "toolRequest",
        id: update.toolCallId,
        name: update.title,
        ...identity,
        arguments: rawInputToArguments(update.rawInput),
        status: "executing",
        ...toolCallUpdatePatch(update),
        startedAt: Date.now(),
      };
      store.setStreamingMessageId(sessionId, messageId);
      store.appendToStreamingMessage(sessionId, toolRequest);
      break;
    }

    case "tool_call_update": {
      const messageId = ensureLiveAssistantMessage(sessionId, gooseSessionId);
      const identity = getToolCallIdentity(update);

      const patch = toolCallUpdatePatch(update);
      if (
        update.title ||
        Object.keys(identity).length > 0 ||
        Object.keys(patch).length > 0
      ) {
        store.updateMessage(sessionId, messageId, (msg) => ({
          ...msg,
          content: msg.content.map((c) =>
            c.type === "toolRequest" && c.id === update.toolCallId
              ? {
                  ...c,
                  ...(update.title ? { name: update.title } : {}),
                  ...identity,
                  ...patch,
                }
              : c,
          ),
        }));
      }

      if (update.status === "completed" || update.status === "failed") {
        const toolCallStatus = toolCallStatusFromUpdate(update.status);
        const streamingMessage = store.messagesBySession[sessionId]?.find(
          (m) => m.id === messageId,
        );
        const toolRequest = streamingMessage
          ? findLatestUnpairedToolRequest(streamingMessage.content)
          : null;

        store.updateMessage(sessionId, messageId, (msg) => ({
          ...msg,
          content: msg.content.map((block) =>
            block.type === "toolRequest" && block.id === update.toolCallId
              ? {
                  ...block,
                  ...identity,
                  ...toolCallUpdatePatch(update),
                  status: toolCallStatus,
                }
              : block,
          ),
        }));

        const resultText = extractToolResultText(update);
        const toolResponse: ToolResponseContent = {
          type: "toolResponse",
          id: update.toolCallId,
          name: toolRequest?.name ?? "",
          result: resultText,
          structuredContent: extractToolStructuredContent(update),
          isError: update.status === "failed",
        };
        store.setStreamingMessageId(sessionId, messageId);
        store.appendToStreamingMessage(sessionId, toolResponse);
        if (update.status === "completed") {
          attachMcpAppPayload(
            sessionId,
            update.toolCallId,
            toolRequest?.name ?? update.title ?? "",
            update,
            false,
            {
              gooseSessionId,
            },
          );
        }
      }
      break;
    }

    case "session_info_update":
    case "config_option_update":
    case "usage_update":
      handleShared(sessionId, gooseSessionId, localSessionId, update);
      break;

    default:
      break;
  }
}

function handleShared(
  sessionId: string,
  gooseSessionId: string,
  localSessionId: string | null,
  update: SessionUpdate,
): void {
  switch (update.sessionUpdate) {
    case "session_info_update": {
      const info = update as SessionUpdate & {
        sessionUpdate: "session_info_update";
      };
      if ("title" in info && info.title) {
        const session = useChatSessionStore.getState().getSession(sessionId);
        if (session && !session.userSetName) {
          useChatSessionStore
            .getState()
            .updateSession(sessionId, { title: info.title as string });
        }
      }
      break;
    }

    case "config_option_update": {
      const configUpdate = update as SessionUpdate & {
        sessionUpdate: "config_option_update";
      };
      if ("options" in configUpdate && Array.isArray(configUpdate.options)) {
        const modelOption = configUpdate.options.find(
          (opt: { category?: string; kind?: Record<string, unknown> }) =>
            opt.category === "model",
        );
        if (modelOption?.kind?.type === "select") {
          const select = modelOption.kind;
          const currentModelId = select.currentValue;
          const availableModels: Array<{ id: string; name: string }> = [];

          if (select.options?.type === "ungrouped") {
            for (const v of select.options.values) {
              availableModels.push({ id: v.value, name: v.name });
            }
          } else if (select.options?.type === "grouped") {
            for (const group of select.options.groups) {
              for (const v of group.options) {
                availableModels.push({ id: v.value, name: v.name });
              }
            }
          }

          const currentModelName =
            availableModels.find((m) => m.id === currentModelId)?.name ??
            currentModelId;

          const sessionStore = useChatSessionStore.getState();
          sessionStore.updateSession(sessionId, {
            modelId: currentModelId,
            modelName: currentModelName,
          });
        }
      }
      break;
    }

    case "usage_update": {
      const usage = update as SessionUpdate & { sessionUpdate: "usage_update" };

      if (!localSessionId) {
        pendingUsageUpdates.set(gooseSessionId, {
          accumulatedTotal: usage.used,
          contextLimit: usage.size,
        });
        break;
      }

      useChatStore.getState().updateTokenState(localSessionId, {
        accumulatedTotal: usage.used,
        contextLimit: usage.size,
      });
      break;
    }

    default:
      break;
  }
}

function findStreamingMessageId(sessionId: string): string | null {
  return useChatStore.getState().getSessionRuntime(sessionId)
    .streamingMessageId;
}

function ensureLiveAssistantMessage(
  sessionId: string,
  gooseSessionId: string,
  preferredMessageId?: string | null,
): string {
  const store = useChatStore.getState();
  const existingStreamingMessageId = findStreamingMessageId(sessionId);
  const messages = store.messagesBySession[sessionId] ?? [];

  if (
    existingStreamingMessageId &&
    messages.some((message) => message.id === existingStreamingMessageId)
  ) {
    return existingStreamingMessageId;
  }

  const messageId =
    preferredMessageId ??
    presetMessageIds.get(gooseSessionId) ??
    existingStreamingMessageId ??
    crypto.randomUUID();

  if (!messages.some((message) => message.id === messageId)) {
    store.addMessage(sessionId, {
      id: messageId,
      role: "assistant",
      created: Date.now(),
      content: [],
      metadata: {
        userVisible: true,
        agentVisible: true,
        completionStatus: "inProgress",
      },
    });
  }

  store.setPendingAssistantProvider(sessionId, null);
  store.setStreamingMessageId(sessionId, messageId);
  clearActiveMessageId(gooseSessionId);

  return messageId;
}

export function clearMessageTracking(): void {
  presetMessageIds.clear();
  pendingUsageUpdates.clear();
  clearReplayAssistantTracking();
}

const handler: AcpNotificationHandler = {
  handleSessionNotification,
};

export default handler;
