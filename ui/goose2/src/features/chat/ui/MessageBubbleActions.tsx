import { useTranslation } from "react-i18next";
import { Copy, Check, RotateCcw, Pencil } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";
import { useCopyToClipboard } from "@/hooks/use-copy-to-clipboard";
import { MessageActions, MessageAction } from "@/shared/ui/ai-elements/message";

function CopyAction({
  copied,
  onCopy,
}: {
  copied: boolean;
  onCopy: () => void;
}) {
  const { t } = useTranslation(["chat", "common"]);

  return (
    <MessageAction
      size="xs"
      variant="ghost-light"
      className={cn(
        "text-muted-foreground",
        copied && "bg-accent text-foreground hover:bg-accent active:bg-accent",
      )}
      tooltip={copied ? t("message.copied") : t("common:actions.copy")}
      onClick={onCopy}
    >
      {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
    </MessageAction>
  );
}

interface MessageBubbleActionsProps {
  isUser: boolean;
  messageId: string;
  textContent: string;
  timestamp: ReactNode;
  onRetryMessage?: (messageId: string) => void;
  onEditMessage?: (messageId: string) => void;
}

export function MessageBubbleActions({
  isUser,
  messageId,
  textContent,
  timestamp,
  onRetryMessage,
  onEditMessage,
}: MessageBubbleActionsProps) {
  const { t } = useTranslation(["chat", "common"]);
  const { isCopied, copyToClipboard } = useCopyToClipboard();

  return (
    <div
      data-role="message-actions"
      data-copy-confirmed={isCopied ? "true" : "false"}
      className={cn(
        "absolute bottom-0 transition-opacity duration-150 ease-out",
        "opacity-0 pointer-events-none",
        "group-hover:animate-in group-hover:slide-in-from-top-2 group-hover:opacity-100 group-hover:pointer-events-auto",
        "group-focus-within:animate-in group-focus-within:slide-in-from-top-2 group-focus-within:opacity-100 group-focus-within:pointer-events-auto",
        isCopied && "opacity-100 pointer-events-auto",
        isUser ? "right-0" : "left-0",
      )}
    >
      <MessageActions className="pt-0">
        {isUser && timestamp}
        {textContent && (
          <CopyAction
            copied={isCopied}
            onCopy={() => copyToClipboard(textContent)}
          />
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
    </div>
  );
}
