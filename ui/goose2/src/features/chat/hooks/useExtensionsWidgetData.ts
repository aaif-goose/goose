import { useEffect, useMemo, useRef, useState } from "react";
import {
  listExtensions,
  listSessionExtensions,
} from "@/features/extensions/api/extensions";
import type {
  ExtensionEntry,
  SessionExtensionStatus,
} from "@/features/extensions/types";
import { getUsedSessionExtensions } from "@/features/extensions/lib/extensionUsage";
import {
  getToolUsageSnapshot,
  mergeExtensionStatuses,
  type ToolUsageSnapshot,
} from "@/features/extensions/lib/extensionsWidgetData";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../stores/chatStore";

const EMPTY_MESSAGES: Message[] = [];

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
