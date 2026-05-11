import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Check, FileText, FolderClosed } from "lucide-react";
import { IconRobot } from "@tabler/icons-react";
import { openPath } from "@tauri-apps/plugin-opener";
import { cn } from "@/shared/lib/cn";
import { useLocaleFormatting } from "@/shared/i18n";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useCopyToClipboard } from "@/hooks/use-copy-to-clipboard";
import { getCatalogEntryFromEntries } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import {
  getProviderIcon,
  formatProviderLabel,
} from "@/shared/ui/icons/ProviderIcons";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import { MessageResponse } from "@/shared/ui/ai-elements/message";
import {
  Reasoning,
  ReasoningTrigger,
  ReasoningContent,
} from "@/shared/ui/ai-elements/reasoning";
import type { McpAppMessageHandler } from "./mcpAppTypes";
import { ToolChainCards, type ToolChainItem } from "./ToolChainCards";
import { ClickableImage } from "./ClickableImage";
import { McpAppView } from "./McpAppView";
import { useArtifactLinkHandler } from "@/features/chat/hooks/useArtifactLinkHandler";
import type {
  Message,
  MessageAttachment,
  MessageContent,
  TextContent,
  ImageContent,
  McpAppContent,
  ToolRequestContent,
  ToolResponseContent,
  ThinkingContent,
  ReasoningContent as ReasoningContentType,
  SystemNotificationContent,
} from "@/shared/types/messages";
import { MessageBubbleActions } from "./MessageBubbleActions";
import { MessageMetadataChip } from "./MessageMetadataChip";

function MessageAttachmentRow({
  attachment,
}: {
  attachment: MessageAttachment;
}) {
  const { t } = useTranslation("chat");
  const Icon = attachment.type === "directory" ? FolderClosed : FileText;
  const canOpen = Boolean(attachment.path);

  return (
    <button
      type="button"
      onClick={() => {
        if (!attachment.path) {
          return;
        }
        void openPath(attachment.path);
      }}
      disabled={!canOpen}
      className={cn(
        "flex max-w-full items-center gap-2 rounded-full border border-border bg-muted/40 px-3 py-1.5 text-xs text-foreground",
        canOpen
          ? "cursor-pointer hover:bg-muted/70"
          : "cursor-default opacity-80",
      )}
      aria-label={
        canOpen
          ? t("attachments.open", { name: attachment.name })
          : attachment.name
      }
      title={attachment.path ?? attachment.name}
    >
      <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate">{attachment.name}</span>
    </button>
  );
}

interface MessageBubbleProps {
  message: Message;
  isStreaming?: boolean;
  onCopy?: () => void;
  onRetryMessage?: (messageId: string) => void;
  onEditMessage?: (messageId: string) => void;
  onSendMcpAppMessage?: McpAppMessageHandler;
  onMcpAppAutoScroll?: (element: HTMLElement | null) => void;
}

interface ContentSection {
  key: string;
  type: "single" | "toolChain";
  items: MessageContent[] | ToolChainItem[];
}

function filterUserVisibleContent(content: MessageContent[]): MessageContent[] {
  return content.filter((b) => {
    const aud = "annotations" in b ? b.annotations?.audience : undefined;
    return !aud || aud.length === 0 || aud.includes("user");
  });
}

function findMatchingToolChainIndex(
  items: ToolChainItem[],
  response: ToolResponseContent,
): number {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (!item.request || item.response) {
      continue;
    }
    if (item.request.id === response.id) {
      return index;
    }
  }

  if (!response.name) {
    for (let index = items.length - 1; index >= 0; index -= 1) {
      const item = items[index];
      if (item.request && !item.response) {
        return index;
      }
    }
  }

  return -1;
}

function groupContentSections(content: MessageContent[]): ContentSection[] {
  const sections: ContentSection[] = [];
  let currentToolChain: ToolChainItem[] = [];

  const flushToolChain = () => {
    if (currentToolChain.length > 0) {
      sections.push({
        key: currentToolChain.map((item) => item.key).join(":"),
        type: "toolChain",
        items: [...currentToolChain],
      });
      currentToolChain = [];
    }
  };

  for (const [index, block] of content.entries()) {
    if (block.type === "toolRequest") {
      currentToolChain.push({
        key: `tool-request-${block.id}-${index}`,
        request: block,
      });
      continue;
    }

    if (block.type === "toolResponse") {
      const matchingIndex = findMatchingToolChainIndex(currentToolChain, block);
      if (matchingIndex !== -1) {
        const requestName = currentToolChain[matchingIndex].request?.name ?? "";
        currentToolChain[matchingIndex] = {
          ...currentToolChain[matchingIndex],
          response: {
            ...block,
            name: block.name || requestName,
          },
        };
        continue;
      }
      currentToolChain.push({
        key: `tool-response-${block.id}-${index}`,
        response: block,
      });
      continue;
    }

    flushToolChain();
    sections.push({
      key: `${block.type}-${"id" in block ? String(block.id) : index}`,
      type: "single",
      items: [block],
    });
  }

  flushToolChain();

  return sections;
}

function renderContentBlock(
  content: MessageContent,
  index: number,
  options: {
    defaultImageAlt: string;
    redactedThinking: string;
    contentBlocks: MessageContent[];
    onSendMcpAppMessage?: McpAppMessageHandler;
    onMcpAppAutoScroll?: (element: HTMLElement | null) => void;
  },
  isStreamingMsg?: boolean,
  isUserMessage?: boolean,
) {
  switch (content.type) {
    case "text": {
      const tc = content as TextContent;
      if (isUserMessage) {
        if (!tc.text.trim()) {
          return null;
        }
        return (
          <p key={`text-${index}`} className="whitespace-pre-wrap break-words">
            {tc.text}
          </p>
        );
      }
      return (
        <MessageResponse key={`text-${index}`} isAnimating={isStreamingMsg}>
          {tc.text}
        </MessageResponse>
      );
    }
    case "image": {
      const ic = content as ImageContent;
      const src = ic.uri ?? `data:${ic.mimeType};base64,${ic.data}`;
      return (
        <ClickableImage
          key={`image-${index}`}
          src={src}
          alt={options.defaultImageAlt}
        />
      );
    }
    case "toolRequest":
    case "toolResponse":
      // Handled by groupContentSections toolChain rendering
      return null;
    case "mcpApp": {
      const mcpApp = content as McpAppContent;
      const matchingToolInput = options.contentBlocks.find(
        (block): block is ToolRequestContent =>
          block.type === "toolRequest" &&
          block.id === mcpApp.payload.toolCallId,
      );
      const matchingToolResponse = options.contentBlocks.find(
        (block): block is ToolResponseContent =>
          block.type === "toolResponse" &&
          block.id === mcpApp.payload.toolCallId,
      );

      return (
        <McpAppView
          key={`mcp-app-${index}`}
          payload={mcpApp.payload}
          toolInput={matchingToolInput?.arguments}
          toolResponse={matchingToolResponse}
          onSendMessage={options.onSendMcpAppMessage}
          onAutoScrollRequest={options.onMcpAppAutoScroll}
        />
      );
    }
    case "thinking":
    case "reasoning": {
      const text = (content as ThinkingContent | ReasoningContentType).text;
      return (
        <Reasoning
          key={`${content.type}-${index}`}
          isStreaming={isStreamingMsg}
          defaultOpen={false}
        >
          <ReasoningTrigger />
          <ReasoningContent>{text}</ReasoningContent>
        </Reasoning>
      );
    }
    case "redactedThinking":
      return (
        <div
          key={`redacted-${index}`}
          className="text-xs italic text-muted-foreground"
        >
          {options.redactedThinking}
        </div>
      );
    case "systemNotification": {
      const sn = content as SystemNotificationContent;
      const isError = sn.notificationType === "error";
      const isCompaction = sn.notificationType === "compaction";
      return (
        <div
          key={`notification-${index}`}
          className={cn(
            "rounded-md border p-2 text-xs",
            isError
              ? "border-danger/30 bg-danger/10 text-danger"
              : isCompaction
                ? "inline-flex items-center justify-center gap-2 border-success/30 bg-success/10 font-medium text-success"
                : "border-border bg-accent text-muted-foreground",
          )}
        >
          {isCompaction ? <Check className="size-3.5 shrink-0" /> : null}
          <span>{sn.text}</span>
        </div>
      );
    }
    default:
      return null;
  }
}

export const MessageBubble = memo(function MessageBubble({
  message,
  isStreaming,
  onRetryMessage,
  onEditMessage,
  onSendMcpAppMessage,
  onMcpAppAutoScroll,
}: MessageBubbleProps) {
  const { t } = useTranslation(["chat", "common"]);
  const { formatDate } = useLocaleFormatting();
  const { role, content: rawContent, created } = message;
  // Only user messages carry annotated blocks; skip the filter for others.
  const content =
    role === "user" ? filterUserVisibleContent(rawContent) : rawContent;
  const { handleContentClick, pathNotice } = useArtifactLinkHandler();
  const persona = useAgentStore((state) =>
    message.metadata?.personaId
      ? state.getPersonaById(message.metadata.personaId)
      : undefined,
  );
  const { isCopied: isCopyConfirmed, copyToClipboard } = useCopyToClipboard();
  const personaAvatarUrl = useAvatarSrc(persona?.avatar);
  const catalogEntries = useProviderCatalogStore((state) => state.entries);

  // Skip empty user bubbles (all blocks filtered as assistant-only).
  if (role === "user" && content.length === 0) return null;

  const textContent = content
    .filter((c): c is TextContent => c.type === "text")
    .map((c) => c.text)
    .join("\n");
  const hasMcpApp = content.some((block) => block.type === "mcpApp");

  if (role === "system") {
    return (
      <div className="flex justify-center px-4 py-2">
        <div className="w-full max-w-md text-center text-xs text-muted-foreground">
          {content.map((c, i) =>
            renderContentBlock(c, i, {
              defaultImageAlt: t("message.defaultImageAlt"),
              redactedThinking: t("message.redactedThinking"),
              contentBlocks: content,
            }),
          )}
        </div>
      </div>
    );
  }
  const isUser = role === "user";
  const assistantProviderId = message.metadata?.providerId;
  const assistantProviderName = assistantProviderId
    ? (getCatalogEntryFromEntries(catalogEntries, assistantProviderId)
        ?.displayName ?? formatProviderLabel(assistantProviderId))
    : undefined;
  const assistantDisplayName =
    message.metadata?.personaName ??
    persona?.displayName ??
    assistantProviderName;
  const assistantProviderIcon = assistantProviderId
    ? getProviderIcon(assistantProviderId, "size-3.5")
    : null;
  const showAssistantIdentity = Boolean(
    !isUser &&
      (assistantDisplayName || personaAvatarUrl || assistantProviderIcon),
  );
  const messageAttachments = message.metadata?.attachments ?? [];
  const messageChips = message.metadata?.chips ?? [];
  const timestamp = (
    <span
      data-role="message-timestamp"
      className="shrink-0 whitespace-nowrap px-1 text-[10px] text-muted-foreground"
    >
      {formatDate(created, {
        hour: "2-digit",
        minute: "2-digit",
      })}
    </span>
  );

  return (
    <div
      className={cn(
        "flex px-4 py-1",
        "animate-in fade-in duration-200 motion-reduce:animate-none",
        isUser ? "ml-auto flex-row-reverse gap-3" : "flex-row",
      )}
      data-role={isUser ? "user-message" : "assistant-message"}
    >
      <div
        className={cn(
          "group relative min-w-0 flex flex-col gap-1 pb-8",
          isUser
            ? "max-w-[640px] items-end"
            : hasMcpApp
              ? "w-full items-start"
              : "max-w-[85%] items-start",
        )}
      >
        {showAssistantIdentity ? (
          <div className="mb-0.5 flex items-center gap-1 text-xs">
            {personaAvatarUrl ? (
              <img
                src={personaAvatarUrl}
                alt=""
                className="h-5 w-5 rounded-full"
              />
            ) : assistantProviderIcon ? (
              <span className="flex h-5 w-5 items-center justify-center">
                {assistantProviderIcon}
              </span>
            ) : (
              <span className="flex h-5 w-5 items-center justify-center">
                <IconRobot size={14} className="text-muted-foreground" />
              </span>
            )}
            {assistantDisplayName ? (
              <span className="font-normal text-foreground">
                {assistantDisplayName}
              </span>
            ) : null}
          </div>
        ) : null}

        {/* biome-ignore lint/a11y/useKeyWithClickEvents: delegated link handler */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: delegated link handler */}
        <div
          className={cn(
            "w-full min-w-0 text-sm leading-relaxed",
            isUser && "rounded-2xl bg-muted p-3",
          )}
          onClick={handleContentClick}
        >
          {isUser && messageChips.length > 0 && (
            <div className="mb-1.5 flex flex-wrap gap-1.5">
              {messageChips.map((chip) => (
                <MessageMetadataChip
                  key={`${chip.type}-${chip.label}`}
                  chip={chip}
                />
              ))}
            </div>
          )}
          {messageAttachments.length > 0 && (
            <div className="mb-2 flex flex-wrap gap-2">
              {messageAttachments.map((attachment) => (
                <MessageAttachmentRow
                  key={`${attachment.type}-${attachment.path ?? attachment.name}`}
                  attachment={attachment}
                />
              ))}
            </div>
          )}
          {groupContentSections(content).map((section, sectionIdx) => {
            if (section.type === "toolChain") {
              const toolItems = section.items as ToolChainItem[];
              return <ToolChainCards key={section.key} toolItems={toolItems} />;
            }
            const block = section.items[0] as MessageContent;
            return (
              <div key={`${message.id}-${section.key}`}>
                {renderContentBlock(
                  block,
                  sectionIdx,
                  {
                    defaultImageAlt: t("message.defaultImageAlt"),
                    redactedThinking: t("message.redactedThinking"),
                    contentBlocks: content,
                    onSendMcpAppMessage,
                    onMcpAppAutoScroll,
                  },
                  isStreaming,
                  isUser,
                )}
              </div>
            );
          })}
          {pathNotice && (
            <p className="mt-2 text-xs text-destructive" role="status">
              {pathNotice}
            </p>
          )}
        </div>

        <div
          data-role="message-actions"
          data-copy-confirmed={isCopyConfirmed ? "true" : "false"}
          className={cn(
            "absolute bottom-0 transition-opacity duration-150 ease-out",
            "opacity-0 pointer-events-none",
            "group-hover:animate-in group-hover:slide-in-from-top-2 group-hover:opacity-100 group-hover:pointer-events-auto",
            "group-focus-within:animate-in group-focus-within:slide-in-from-top-2 group-focus-within:opacity-100 group-focus-within:pointer-events-auto",
            isCopyConfirmed && "opacity-100 pointer-events-auto",
            isUser ? "right-0" : "left-0",
          )}
        >
          <MessageBubbleActions
            isUser={isUser}
            messageId={message.id}
            timestamp={timestamp}
            textContent={textContent}
            copied={isCopyConfirmed}
            onCopy={() => copyToClipboard(textContent)}
            onRetryMessage={onRetryMessage}
            onEditMessage={onEditMessage}
          />
        </div>
      </div>
    </div>
  );
});
