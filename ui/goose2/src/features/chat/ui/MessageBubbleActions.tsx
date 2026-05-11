import type { ReactNode } from "react";
import { Check, Copy, Pencil, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { MessageAction, MessageActions } from "@/shared/ui/ai-elements/message";

interface MessageBubbleActionsProps {
  isUser: boolean;
  messageId: string;
  timestamp: ReactNode;
  textContent: string;
  copied: boolean;
  onCopy: () => void;
  onRetryMessage?: (messageId: string) => void;
  onEditMessage?: (messageId: string) => void;
}

export function MessageBubbleActions({
  isUser,
  messageId,
  timestamp,
  textContent,
  copied,
  onCopy,
  onRetryMessage,
  onEditMessage,
}: MessageBubbleActionsProps) {
  const { t } = useTranslation(["chat", "common"]);

  return (
    <MessageActions className="pt-0">
      {isUser && timestamp}
      {textContent && (
        <MessageAction
          size="xs"
          variant="ghost-light"
          className={cn(
            "text-muted-foreground",
            copied &&
              "bg-accent text-foreground hover:bg-accent active:bg-accent",
          )}
          tooltip={copied ? t("message.copied") : t("common:actions.copy")}
          onClick={onCopy}
        >
          {copied ? (
            <Check className="size-3.5" />
          ) : (
            <Copy className="size-3.5" />
          )}
        </MessageAction>
      )}
      {!isUser && onRetryMessage && (
        <MessageAction
          size="xs"
          variant="ghost-light"
          className="text-muted-foreground"
          tooltip={t("common:actions.retry")}
          onClick={() => onRetryMessage(messageId)}
        >
          <RotateCcw className="size-3.5" />
        </MessageAction>
      )}
      {isUser && onEditMessage && (
        <MessageAction
          size="xs"
          variant="ghost-light"
          className="text-muted-foreground"
          tooltip={t("common:actions.edit")}
          onClick={() => onEditMessage(messageId)}
        >
          <Pencil className="size-3.5" />
        </MessageAction>
      )}
      {!isUser && timestamp}
    </MessageActions>
  );
}
