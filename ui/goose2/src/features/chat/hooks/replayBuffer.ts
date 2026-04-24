/**
 * Replay buffering for session history loading.
 *
 * During session history replay, the backend fires individual Tauri events for
 * every historical message. Previously each event called store.addMessage(),
 * creating a new Zustand state object and triggering a React re-render of the
 * full message list — O(N²) work for N messages.
 *
 * Instead, replay events now accumulate messages in this module-level buffer.
 * When the session finishes loading (loadingSessionIds removes the id), the
 * buffer is flushed as a single store.setMessages() call — O(1) re-render.
 */
import type {
  Message,
  MessageContent,
  ToolRequestContent,
  ToolResponseContent,
} from "@/shared/types/messages";

const replayBuffers = new Map<string, Message[]>();

export function ensureReplayBuffer(sessionId: string): Message[] {
  let buffer = replayBuffers.get(sessionId);
  if (!buffer) {
    buffer = [];
    replayBuffers.set(sessionId, buffer);
  }
  return buffer;
}

export function getBufferedMessage(
  sessionId: string,
  messageId: string,
): Message | undefined {
  return replayBuffers.get(sessionId)?.find((m) => m.id === messageId);
}

export function getAndDeleteReplayBuffer(
  sessionId: string,
): Message[] | undefined {
  const buffer = replayBuffers.get(sessionId);
  replayBuffers.delete(sessionId);
  return buffer;
}

/** Discard the replay buffer for a session without returning it. */
export function clearReplayBuffer(sessionId: string): void {
  replayBuffers.delete(sessionId);
}

export function findLatestUnpairedToolRequest(
  content: MessageContent[],
): ToolRequestContent | null {
  for (let index = content.length - 1; index >= 0; index -= 1) {
    const block = content[index];
    if (block?.type !== "toolRequest") {
      continue;
    }

    const alreadyHasResponse = content.some(
      (candidate): candidate is ToolResponseContent =>
        candidate.type === "toolResponse" && candidate.id === block.id,
    );

    if (!alreadyHasResponse) {
      return block;
    }
  }

  return null;
}
