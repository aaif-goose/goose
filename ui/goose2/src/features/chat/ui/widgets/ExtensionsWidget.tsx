import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconApps } from "@tabler/icons-react";
import {
  listExtensions,
  listSessionExtensions,
} from "@/features/extensions/api/extensions";
import {
  getDisplayName,
  type ExtensionEntry,
  type SessionExtensionStatus,
} from "@/features/extensions/types";
import { getUsedSessionExtensions } from "@/features/extensions/lib/extensionUsage";
import { normalizeExtensionKey } from "@/features/extensions/lib/extensionKeys";
import { cn } from "@/shared/lib/cn";
import type { Message, ToolRequestContent } from "@/shared/types/messages";
import { useChatStore } from "../../stores/chatStore";
import { Widget } from "./Widget";

interface ExtensionsWidgetProps {
  sessionId: string;
}

const EMPTY_MESSAGES: Message[] = [];

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

function toolRequestOwnerKey(toolRequest: ToolRequestContent): string {
  if (toolRequest.extensionName) {
    return normalizeExtensionKey(toolRequest.extensionName);
  }

  const toolName = toolRequest.toolName ?? toolRequest.name;
  const [owner] = toolName.split("__");
  if (owner && owner !== toolName) {
    return normalizeExtensionKey(owner);
  }
  return normalizeExtensionKey(toolName);
}

function ExtensionRow({ extension }: { extension: SessionExtensionStatus }) {
  const { t } = useTranslation("chat");
  const displayName = getDisplayName(extension);
  const isConnected = extension.status === "connected";
  const isAvailable = extension.status === "available";
  const isUnavailable = extension.status === "unavailable";
  const toolCount = extension.tools.length;

  return (
    <div className="flex min-w-0 items-start gap-2" title={extension.error}>
      <span
        className={cn(
          "mt-1.5 size-1.5 shrink-0 rounded-full",
          isConnected || isAvailable
            ? "bg-green-500"
            : isUnavailable
              ? "bg-muted-foreground"
              : "bg-amber-500",
        )}
      />
      <div className="min-w-0 flex-1">
        <div className="truncate text-xs text-foreground">{displayName}</div>
        <div
          className={cn(
            "mt-0.5 truncate text-[11px]",
            isConnected || isAvailable || isUnavailable
              ? "text-foreground-subtle"
              : "text-amber-600",
          )}
        >
          {isConnected
            ? t("contextPanel.widgets.statusConnected")
            : isAvailable
              ? t("contextPanel.widgets.statusAvailable")
              : isUnavailable
                ? t("contextPanel.widgets.statusUnavailable")
                : t("contextPanel.widgets.statusFailed")}
          {isConnected && toolCount > 0
            ? ` · ${t("contextPanel.widgets.toolCount", { count: toolCount })}`
            : null}
        </div>
      </div>
    </div>
  );
}

export function ExtensionsWidget({ sessionId }: ExtensionsWidgetProps) {
  const { t } = useTranslation("chat");
  const [extensions, setExtensions] = useState<SessionExtensionStatus[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const messages = useChatStore(
    (s) => s.messagesBySession[sessionId] ?? EMPTY_MESSAGES,
  );

  const toolOwnerSignature = useMemo(() => {
    const owners = new Set<string>();
    for (const message of messages) {
      for (const content of message.content) {
        if (content.type === "toolRequest") {
          owners.add(toolRequestOwnerKey(content));
        }
      }
    }
    return Array.from(owners).sort().join("|");
  }, [messages]);

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

  const used = useMemo(
    () => getUsedSessionExtensions(extensions, messages),
    [extensions, messages],
  );

  const renderSection = (sectionExtensions: SessionExtensionStatus[]) => {
    if (sectionExtensions.length === 0) return null;
    return (
      <div className="space-y-2">
        {sectionExtensions.map((ext) => (
          <ExtensionRow key={ext.config_key} extension={ext} />
        ))}
      </div>
    );
  };

  return (
    <Widget
      title={t("contextPanel.widgets.extensionsUsedTitle")}
      icon={<IconApps className="size-3.5" />}
      flush
    >
      {isLoading ? (
        <div className="space-y-2 px-3 py-2.5">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-4 animate-pulse rounded bg-muted/40" />
          ))}
        </div>
      ) : used.length === 0 ? (
        <p className="px-3 py-2.5 text-xs text-foreground-subtle">
          {t("contextPanel.empty.noExtensions")}
        </p>
      ) : (
        <div>
          <div className="max-h-56 space-y-3 overflow-y-auto px-3 py-2">
            {renderSection(used)}
          </div>
        </div>
      )}
    </Widget>
  );
}
