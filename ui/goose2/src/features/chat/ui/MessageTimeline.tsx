import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { useLocaleFormatting } from "@/shared/i18n";
import { MessageBubble } from "./MessageBubble";
import { getTextContent, type Message } from "@/shared/types/messages";

interface MessageTimelineProps {
  messages: Message[];
  streamingMessageId?: string | null;
  scrollTargetMessageId?: string | null;
  scrollTargetQuery?: string | null;
  onScrollTargetHandled?: (messageId: string) => void;
  onRetryMessage?: (messageId: string) => void;
  onEditMessage?: (messageId: string) => void;
  className?: string;
}

function isSameDay(a: number, b: number): boolean {
  const da = new Date(a);
  const db = new Date(b);
  return (
    da.getFullYear() === db.getFullYear() &&
    da.getMonth() === db.getMonth() &&
    da.getDate() === db.getDate()
  );
}

function formatDateSeparator(
  timestamp: number,
  todayLabel: string,
  yesterdayLabel: string,
  formatDate: (
    value: Date | string | number,
    options?: Intl.DateTimeFormatOptions,
  ) => string,
): string {
  const now = new Date();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);

  if (isSameDay(timestamp, now.getTime())) return todayLabel;
  if (isSameDay(timestamp, yesterday.getTime())) return yesterdayLabel;

  return formatDate(timestamp, {
    weekday: "long",
    month: "short",
    day: "numeric",
  });
}

export function MessageTimeline({
  messages,
  streamingMessageId,
  scrollTargetMessageId,
  scrollTargetQuery,
  onScrollTargetHandled,
  onRetryMessage,
  onEditMessage,
  className,
}: MessageTimelineProps) {
  const { t } = useTranslation("chat");
  const { formatDate } = useLocaleFormatting();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const messageRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const isNearBottomRef = useRef(true);
  const [pulsingMessageId, setPulsingMessageId] = useState<string | null>(null);
  const visibleMessages = messages.filter(
    (m) =>
      m.metadata?.userVisible !== false &&
      !(
        m.role === "assistant" &&
        m.content.length === 0 &&
        m.metadata?.completionStatus === "inProgress"
      ),
  );
  const resolvedScrollTargetMessageId = useMemo(() => {
    if (scrollTargetMessageId) {
      const exactMatch = visibleMessages.find(
        (message) => message.id === scrollTargetMessageId,
      );
      if (exactMatch) {
        return exactMatch.id;
      }
    }

    const trimmedQuery = scrollTargetQuery?.trim().toLocaleLowerCase();
    if (!trimmedQuery) {
      return null;
    }

    const textMatch = visibleMessages.find((message) =>
      getTextContent(message).toLocaleLowerCase().includes(trimmedQuery),
    );
    return textMatch?.id ?? null;
  }, [scrollTargetMessageId, scrollTargetQuery, visibleMessages]);

  // Use scrollTo instead of scrollIntoView to avoid scrolling parent/document-level ancestors.
  // biome-ignore lint/correctness/useExhaustiveDependencies: refs are stable and don't need to be in deps
  useEffect(() => {
    if (isNearBottomRef.current && containerRef.current) {
      containerRef.current.scrollTo({
        top: containerRef.current.scrollHeight,
        behavior: "smooth",
      });
    }
  }, [messages, streamingMessageId]);

  useEffect(() => {
    if (!resolvedScrollTargetMessageId) {
      return;
    }

    const target = messageRefs.current[resolvedScrollTargetMessageId];
    if (!target) {
      return;
    }

    const frame = requestAnimationFrame(() => {
      target.scrollIntoView({
        block: "center",
        behavior: "smooth",
      });
      setPulsingMessageId(resolvedScrollTargetMessageId);
      onScrollTargetHandled?.(resolvedScrollTargetMessageId);
    });

    return () => cancelAnimationFrame(frame);
  }, [onScrollTargetHandled, resolvedScrollTargetMessageId]);

  useEffect(() => {
    if (!pulsingMessageId) {
      return;
    }

    const timer = window.setTimeout(() => {
      setPulsingMessageId((current) =>
        current === pulsingMessageId ? null : current,
      );
    }, 2000);

    return () => window.clearTimeout(timer);
  }, [pulsingMessageId]);

  const handleScroll = () => {
    const container = containerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    isNearBottomRef.current = scrollHeight - scrollTop - clientHeight < 100;
  };

  if (visibleMessages.length === 0) {
    return (
      <div className={cn("flex flex-1 items-center justify-center", className)}>
        <div className="text-center">
          <p className="text-lg font-medium font-display tracking-tight text-muted-foreground">
            {t("timeline.emptyTitle")}
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("timeline.emptyDescription")}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className={cn("flex-1 overflow-y-auto", className)}
      role="log"
      aria-label={t("timeline.ariaLabel")}
      aria-live="polite"
    >
      <div className="mx-auto max-w-3xl py-4">
        {visibleMessages.map((message, index) => {
          const prev = index > 0 ? visibleMessages[index - 1] : null;
          const showDateSeparator =
            !prev || !isSameDay(prev.created, message.created);

          return (
            <div
              key={message.id}
              ref={(el) => {
                messageRefs.current[message.id] = el;
              }}
              className={cn(
                index === 0 ? "mt-0" : "mt-4",
                "rounded-xl transition-[background-color,box-shadow]",
                pulsingMessageId === message.id &&
                  "bg-accent/25 ring-2 ring-accent/35 ring-inset",
              )}
            >
              {showDateSeparator && (
                <div className="my-4 px-4 text-center">
                  <span className="text-[11px] font-medium text-muted-foreground">
                    {formatDateSeparator(
                      message.created,
                      t("timeline.today"),
                      t("timeline.yesterday"),
                      formatDate,
                    )}
                  </span>
                </div>
              )}
              <MessageBubble
                message={message}
                isStreaming={message.id === streamingMessageId}
                onRetryMessage={
                  message.role === "assistant" ? onRetryMessage : undefined
                }
                onEditMessage={
                  message.role === "user" ? onEditMessage : undefined
                }
              />
            </div>
          );
        })}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
