import { useTranslation } from "react-i18next";
import { IconApps } from "@tabler/icons-react";
import {
  getDisplayName,
  type SessionExtensionStatus,
} from "@/features/extensions/types";
import { cn } from "@/shared/lib/cn";
import { useExtensionsWidgetData } from "../../hooks/useExtensionsWidgetData";
import { Widget } from "./Widget";

interface ExtensionsWidgetProps {
  sessionId: string;
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
  const { isLoading, usedExtensions } = useExtensionsWidgetData(sessionId);

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
      ) : usedExtensions.length === 0 ? (
        <p className="px-3 py-2.5 text-xs text-foreground-subtle">
          {t("contextPanel.empty.noExtensions")}
        </p>
      ) : (
        <div>
          <div className="max-h-56 space-y-3 overflow-y-auto px-3 py-2">
            {renderSection(usedExtensions)}
          </div>
        </div>
      )}
    </Widget>
  );
}
