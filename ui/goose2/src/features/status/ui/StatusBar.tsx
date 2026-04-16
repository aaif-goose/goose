import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Bot, Copy, Check } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useLocaleFormatting } from "@/shared/i18n";

interface StatusBarProps {
  modelName?: string;
  sessionId?: string;
  tokenCount?: number;
}

export function StatusBar({
  modelName,
  sessionId,
  tokenCount = 0,
}: StatusBarProps) {
  const { t } = useTranslation("status");
  const { formatNumber } = useLocaleFormatting();
  const [copied, setCopied] = useState(false);

  const handleCopySessionId = () => {
    if (!sessionId) return;
    navigator.clipboard.writeText(sessionId);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={cn(
        "flex h-6 w-full items-center justify-between",
        "bg-background/80 px-3 text-xs text-muted-foreground",
      )}
    >
      <div className="flex items-center gap-1.5 min-w-0">
        <div className="flex items-center gap-1 min-w-0">
          <Bot className="h-3.5 w-3.5 flex-shrink-0 text-muted-foreground" />
          <span className="truncate text-muted-foreground">
            {modelName ?? t("noModel")}
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2">
        {sessionId && (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={handleCopySessionId}
            className="h-auto gap-1 rounded px-1 py-0.5 text-muted-foreground hover:text-muted-foreground"
            title={t("sessionTitle", { id: sessionId })}
          >
            <span className="font-mono">{sessionId.slice(0, 8)}</span>
            {copied ? (
              <Check className="size-2.5" />
            ) : (
              <Copy className="size-2.5" />
            )}
          </Button>
        )}
        {tokenCount > 0 && (
          <span className="text-muted-foreground">
            {t("tokens", {
              count: tokenCount,
              displayCount: formatNumber(tokenCount),
            })}
          </span>
        )}
      </div>
    </div>
  );
}
