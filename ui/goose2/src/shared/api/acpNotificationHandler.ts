import type {
  SessionNotification,
  SessionUpdate,
} from "@agentclientprotocol/sdk";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import {
  ensureReplayBuffer,
  getBufferedMessage,
  findLatestUnpairedToolRequest,
} from "@/features/chat/hooks/replayBuffer";
import type {
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";
import type { AcpNotificationHandler } from "./acpConnection";
import {
  getLocalSessionId,
  subscribeToSessionRegistration,
} from "./acpSessionTracker";
import { perfLog } from "@/shared/lib/perfLog";

// Pre-set message ID for the next live stream per goose session
const presetMessageIds = new Map<string, string>();
const pendingUsageUpdates = new Map<string, SessionUpdate[]>();

function shouldBufferPendingUpdate(update: SessionUpdate): boolean {
  return update.sessionUpdate === "usage_update";
}

function queuePendingUsageUpdate(
  gooseSessionId: string,
  update: SessionUpdate,
): void {
  const pending = pendingUsageUpdates.get(gooseSessionId);
  if (pending) {
    pending.push(update);
    return;
  }
  pendingUsageUpdates.set(gooseSessionId, [update]);
}

function flushPendingUsageUpdates(
  localSessionId: string,
  gooseSessionId: string,
): void {
  const pending = pendingUsageUpdates.get(gooseSessionId);
  if (!pending?.length) {
    return;
  }

  pendingUsageUpdates.delete(gooseSessionId);

  for (const update of pending) {
    if (useChatStore.getState().loadingSessionIds.has(localSessionId)) {
      handleReplay(localSessionId, update);
    } else {
      handleLive(localSessionId, gooseSessionId, update);
    }
  }
}

subscribeToSessionRegistration((localSessionId, gooseSessionId) => {
  flushPendingUsageUpdates(localSessionId, gooseSessionId);
});

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
  const { update } = notification;
  const localSessionId = getLocalSessionId(gooseSessionId);

  if (!localSessionId) {
    if (shouldBufferPendingUpdate(update)) {
      queuePendingUsageUpdate(gooseSessionId, update);
    }
    return;
  }

  const isReplay = useChatStore
    .getState()
    .loadingSessionIds.has(localSessionId);

  if (isReplay) {
    const sid = localSessionId.slice(0, 8);
    let perf = replayPerf.get(localSessionId);
    const now = performance.now();
    if (!perf) {
      perf = { firstAt: now, lastAt: now, count: 0 };
      replayPerf.set(localSessionId, perf);
      perfLog(`[perf:replay] ${sid} first notification received`);
    }
    perf.lastAt = now;
    perf.count += 1;
    handleReplay(localSessionId, update);
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
    handleLive(localSessionId, gooseSessionId, update);
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

function handleReplay(sessionId: string, update: SessionUpdate): void {
  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const messageId = update.messageId ?? crypto.randomUUID();
      const buffer = ensureReplayBuffer(sessionId);
      if (!getBufferedMessage(sessionId, messageId)) {
        buffer.push({
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
      const msg = getBufferedMessage(sessionId, messageId);
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
      const messageId = update.messageId ?? crypto.randomUUID();
      const buffer = ensureReplayBuffer(sessionId);
      const existing = getBufferedMessage(sessionId, messageId);
      if (
        !existing &&
        update.content.type === "text" &&
        "text" in update.content
      ) {
        buffer.push({
          id: messageId,
          role: "user",
          created: Date.now(),
          content: [{ type: "text", text: update.content.text }],
          metadata: { userVisible: true, agentVisible: true },
        });
      } else if (
        existing &&
        update.content.type === "text" &&
        "text" in update.content
      ) {
        const last = existing.content[existing.content.length - 1];
        if (last?.type === "text") {
          (last as { type: "text"; text: string }).text += update.content.text;
        } else {
          existing.content.push({ type: "text", text: update.content.text });
        }
      }
      break;
    }

    case "tool_call": {
      const msg = findMessageInBuffer(sessionId, update.toolCallId);
      if (msg) {
        msg.content.push({
          type: "toolRequest",
          id: update.toolCallId,
          name: update.title,
          arguments: {},
          status: "executing",
          startedAt: Date.now(),
        });
      }
      break;
    }

    case "tool_call_update": {
      const msg = findMessageWithToolCall(sessionId, update.toolCallId);
      if (msg) {
        if (update.title) {
          const tc = msg.content.find(
            (c) => c.type === "toolRequest" && c.id === update.toolCallId,
          );
          if (tc && tc.type === "toolRequest") {
            (tc as ToolRequestContent).name = update.title;
          }
        }
        if (update.status === "completed" || update.status === "failed") {
          const tc = msg.content.find(
            (c) => c.type === "toolRequest" && c.id === update.toolCallId,
          );
          if (tc && tc.type === "toolRequest") {
            const idx = msg.content.indexOf(tc);
            if (idx >= 0) {
              msg.content[idx] = {
                ...tc,
                status: "completed",
              } as ToolRequestContent;
            }
          }
          const resultText = extractToolResultText(update);
          msg.content.push({
            type: "toolResponse",
            id: update.toolCallId,
            name: (tc as ToolRequestContent)?.name ?? "",
            result: resultText,
            isError: update.status === "failed",
          });
        }
      }
      break;
    }

    case "session_info_update":
    case "config_option_update":
    case "usage_update":
      handleShared(sessionId, update);
      break;

    default:
      break;
  }
}

function handleLive(
  sessionId: string,
  gooseSessionId: string,
  update: SessionUpdate,
): void {
  const store = useChatStore.getState();

  switch (update.sessionUpdate) {
    case "agent_message_chunk": {
      const messageId =
        update.messageId ??
        presetMessageIds.get(gooseSessionId) ??
        crypto.randomUUID();
      const existing = store.messagesBySession[sessionId]?.find(
        (m) => m.id === messageId,
      );

      if (!existing) {
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
        store.setPendingAssistantProvider(sessionId, null);
        store.setStreamingMessageId(sessionId, messageId);
      }

      if (update.content.type === "text" && "text" in update.content) {
        store.setStreamingMessageId(sessionId, messageId);
        store.updateStreamingText(sessionId, update.content.text);
      }
      break;
    }

    case "tool_call": {
      const messageId = findStreamingMessageId(sessionId);
      if (!messageId) break;

      const toolRequest: ToolRequestContent = {
        type: "toolRequest",
        id: update.toolCallId,
        name: update.title,
        arguments: {},
        status: "executing",
        startedAt: Date.now(),
      };
      store.setStreamingMessageId(sessionId, messageId);
      store.appendToStreamingMessage(sessionId, toolRequest);
      break;
    }

    case "tool_call_update": {
      const messageId = findStreamingMessageId(sessionId);
      if (!messageId) break;

      if (update.title) {
        store.updateMessage(sessionId, messageId, (msg) => ({
          ...msg,
          content: msg.content.map((c) =>
            c.type === "toolRequest" && c.id === update.toolCallId
              ? { ...c, name: update.title ?? "" }
              : c,
          ),
        }));
      }

      if (update.status === "completed" || update.status === "failed") {
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
              ? { ...block, status: "completed" }
              : block,
          ),
        }));

        const resultText = extractToolResultText(update);
        const toolResponse: ToolResponseContent = {
          type: "toolResponse",
          id: update.toolCallId,
          name: toolRequest?.name ?? "",
          result: resultText,
          isError: update.status === "failed",
        };
        store.setStreamingMessageId(sessionId, messageId);
        store.appendToStreamingMessage(sessionId, toolResponse);
      }
      break;
    }

    case "session_info_update":
    case "config_option_update":
    case "usage_update":
      handleShared(sessionId, update);
      break;

    default:
      break;
  }
}

function handleShared(sessionId: string, update: SessionUpdate): void {
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
            .updateSession(
              sessionId,
              { title: info.title as string },
              { persistOverlay: false },
            );
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
          sessionStore.updateSession(
            sessionId,
            { modelId: currentModelId, modelName: currentModelName },
            { persistOverlay: false },
          );
        }
      }
      break;
    }

    case "usage_update": {
      const usage = update as SessionUpdate & { sessionUpdate: "usage_update" };
      useChatStore.getState().updateTokenState(sessionId, {
        accumulatedTotal: usage.used,
        contextLimit: usage.size,
      });
      break;
    }

    default:
      break;
  }
}

// Helpers

function findStreamingMessageId(sessionId: string): string | null {
  return useChatStore.getState().getSessionRuntime(sessionId)
    .streamingMessageId;
}

function findMessageInBuffer(
  sessionId: string,
  _toolCallId: string,
): ReturnType<typeof getBufferedMessage> {
  const buffer = ensureReplayBuffer(sessionId);
  return buffer[buffer.length - 1];
}

function findMessageWithToolCall(
  sessionId: string,
  toolCallId: string,
): ReturnType<typeof getBufferedMessage> {
  const buffer = ensureReplayBuffer(sessionId);
  for (let i = buffer.length - 1; i >= 0; i--) {
    const msg = buffer[i];
    if (
      msg.content.some((c) => c.type === "toolRequest" && c.id === toolCallId)
    ) {
      return msg;
    }
  }
  return buffer[buffer.length - 1];
}

function extractToolResultText(update: {
  // biome-ignore lint/suspicious/noExplicitAny: ACP SDK ToolCallContent type is complex
  content?: Array<any> | null;
  rawOutput?: unknown;
}): string {
  if (update.content && update.content.length > 0) {
    for (const item of update.content) {
      if (item.type === "content" && item.content?.type === "text") {
        return item.content.text;
      }
    }
  }
  if (update.rawOutput !== undefined && update.rawOutput !== null) {
    return typeof update.rawOutput === "string"
      ? update.rawOutput
      : JSON.stringify(update.rawOutput);
  }
  return "";
}

export function clearMessageTracking(): void {
  presetMessageIds.clear();
}

const handler: AcpNotificationHandler = {
  handleSessionNotification,
};

export default handler;
