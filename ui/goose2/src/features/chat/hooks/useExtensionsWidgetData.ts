import { useEffect, useMemo, useRef, useState } from "react";
import {
  listExtensions,
  listSessionExtensions,
} from "@/features/extensions/api/extensions";
import type {
  ExtensionEntry,
  SessionExtensionStatus,
} from "@/features/extensions/types";
import {
  getToolOwnerSignatureKey,
  getUsedSessionExtensions,
} from "@/features/extensions/lib/extensionUsage";
import type { Message, ToolRequestContent } from "@/shared/types/messages";
import { useChatStore } from "../stores/chatStore";

const EMPTY_MESSAGES: Message[] = [];

interface ToolUsageSnapshot {
  signature: string;
  messages: Message[];
}

function toUnavailableStatus(
  extension: ExtensionEntry,
): SessionExtensionStatus {
  const { enabled: _enabled, ...config } = extension;
  return {
    ...config,
    status: "unavailable",
    tools: [],
  };
}

function mergeExtensionStatuses(
  sessionExtensions: SessionExtensionStatus[],
  configuredExtensions: ExtensionEntry[],
): SessionExtensionStatus[] {
  const byKey = new Map(
    sessionExtensions.map((extension) => [extension.config_key, extension]),
  );
  for (const extension of configuredExtensions) {
    if (!byKey.has(extension.config_key)) {
      byKey.set(extension.config_key, toUnavailableStatus(extension));
    }
  }
  return Array.from(byKey.values());
}

function getToolUsageSnapshot(
  messages: Message[],
  lastSnapshot: ToolUsageSnapshot,
): ToolUsageSnapshot {
  const signatureParts: string[] = [];
  const toolMessages: Message[] = [];

  for (const message of messages) {
    const toolRequests: ToolRequestContent[] = [];
    for (const content of message.content) {
      if (content.type === "toolRequest") {
        const owner = getToolOwnerSignatureKey(content);
        if (owner) {
          signatureParts.push(`${message.id}:${content.id}:${owner}`);
        }
        toolRequests.push(content);
      }
    }
    if (toolRequests.length > 0) {
      toolMessages.push({ ...message, content: toolRequests });
    }
  }

  const signature = signatureParts.join("|");
  if (signature === lastSnapshot.signature) {
    return lastSnapshot;
  }

  return { signature, messages: toolMessages };
}

export function useExtensionsWidgetData(sessionId: string) {
  const [extensions, setExtensions] = useState<SessionExtensionStatus[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const messages = useChatStore(
    (state) => state.messagesBySession[sessionId] ?? EMPTY_MESSAGES,
  );
  const lastToolUsageSnapshot = useRef<ToolUsageSnapshot>({
    signature: "",
    messages: EMPTY_MESSAGES,
  });

  const toolUsageSnapshot = useMemo(() => {
    const nextSnapshot = getToolUsageSnapshot(
      messages,
      lastToolUsageSnapshot.current,
    );
    lastToolUsageSnapshot.current = nextSnapshot;
    return nextSnapshot;
  }, [messages]);
  const toolOwnerSignature = toolUsageSnapshot.signature;

  useEffect(() => {
    let isCurrent = true;

    if (!toolOwnerSignature) {
      setExtensions([]);
      setIsLoading(false);
      return () => {
        isCurrent = false;
      };
    }

    setIsLoading(true);
    Promise.all([
      listSessionExtensions(sessionId).catch(
        () => [] as SessionExtensionStatus[],
      ),
      listExtensions().catch(() => [] as ExtensionEntry[]),
    ])
      .then(([sessionExtensions, configuredExtensions]) => {
        if (isCurrent) {
          setExtensions(
            mergeExtensionStatuses(sessionExtensions, configuredExtensions),
          );
        }
      })
      .catch(() => {
        if (isCurrent) {
          setExtensions([]);
        }
      })
      .finally(() => {
        if (isCurrent) {
          setIsLoading(false);
        }
      });

    return () => {
      isCurrent = false;
    };
  }, [sessionId, toolOwnerSignature]);

  const usedExtensions = useMemo(
    () => getUsedSessionExtensions(extensions, toolUsageSnapshot.messages),
    [extensions, toolUsageSnapshot],
  );

  return {
    isLoading,
    usedExtensions,
  };
}
