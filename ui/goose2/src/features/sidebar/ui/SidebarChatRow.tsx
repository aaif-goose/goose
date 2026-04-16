import { useEffect, useRef, useState } from "react";
import { MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  getDisplaySessionTitle,
  getEditableSessionTitle,
  isSessionTitleUnchanged,
} from "@/features/chat/lib/sessionTitle";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Input } from "@/shared/ui/input";
import { SessionActivityIndicator } from "@/shared/ui/SessionActivityIndicator";

const INACTIVE_CHAT_ROW_CLASS =
  "text-muted-foreground hover:bg-transparent hover:text-foreground group-hover:text-foreground";
const ACTIVE_CHAT_ROW_CLASS =
  "font-medium text-foreground hover:bg-transparent hover:text-foreground";

interface SidebarChatRowProps {
  id: string;
  title: string;
  isActive: boolean;
  isRunning?: boolean;
  hasUnread?: boolean;
  reserveActivitySpace?: boolean;
  className?: string;
  onSelect?: (id: string) => void;
  onRename?: (id: string, nextTitle: string) => void;
  onArchive?: (id: string) => void;
  onMouseEnter?: (e: React.MouseEvent<HTMLElement>) => void;
  activeRef?: (el: HTMLElement | null) => void;
}

export function SidebarChatRow({
  id,
  title,
  isActive,
  isRunning = false,
  hasUnread = false,
  reserveActivitySpace = false,
  className,
  onSelect,
  onRename,
  onArchive,
  onMouseEnter,
  activeRef,
}: SidebarChatRowProps) {
  const { t } = useTranslation(["sidebar", "common"]);
  const [menuOpen, setMenuOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  const displayTitle = getDisplaySessionTitle(
    title,
    t("common:session.defaultTitle"),
  );
  const editableTitle = getEditableSessionTitle(
    title,
    t("common:session.defaultTitle"),
  );
  const [draftTitle, setDraftTitle] = useState(editableTitle);
  const showActivityIndicator = isRunning || hasUnread;
  const shouldReserveActivitySpace =
    reserveActivitySpace || showActivityIndicator;

  useEffect(() => {
    setDraftTitle(editableTitle);
  }, [editableTitle]);

  useEffect(() => {
    if (!editing) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [editing]);

  useEffect(() => {
    if (!activeRef || !isActive || !rowRef.current) {
      return;
    }

    const frame = requestAnimationFrame(() => {
      if (rowRef.current) {
        activeRef(rowRef.current);
      }
    });

    return () => cancelAnimationFrame(frame);
  }, [activeRef, isActive]);

  const startRename = () => {
    setDraftTitle(editableTitle);
    setMenuOpen(false);
    setEditing(true);
  };

  const cancelRename = () => {
    setDraftTitle(editableTitle);
    setEditing(false);
  };

  const commitRename = () => {
    const nextTitle = draftTitle.trim();
    setEditing(false);
    if (
      !nextTitle ||
      isSessionTitleUnchanged(
        nextTitle,
        title,
        t("common:session.defaultTitle"),
      )
    ) {
      return;
    }
    onRename?.(id, nextTitle);
  };

  if (editing) {
    return (
      <div
        className={cn("flex items-center group rounded-md pr-0.5", className)}
      >
        <Input
          ref={inputRef}
          type="text"
          value={draftTitle}
          onChange={(e) => setDraftTitle(e.target.value)}
          onBlur={commitRename}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitRename();
            }
            if (e.key === "Escape") {
              e.preventDefault();
              cancelRename();
            }
          }}
          className="flex-1 min-w-0 px-3 text-[13px] font-light"
          style={{ height: 32 }}
        />
      </div>
    );
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: wrapper div for hover detection, interactive content is the inner Button
    <div
      ref={rowRef}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData("text/x-session-id", id);
        e.dataTransfer.effectAllowed = "move";
        setDragging(true);
      }}
      onDragEnd={() => setDragging(false)}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenuOpen(true);
      }}
      className={cn(
        "relative flex items-center group rounded-md transition-colors duration-200 active:cursor-grabbing",
        dragging && "opacity-40 bg-accent/30",
        className,
      )}
      onMouseEnter={onMouseEnter}
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => onSelect?.(id)}
        onDoubleClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          startRename();
        }}
        title={t("actions.renameHint")}
        className={cn(
          "flex-1 min-w-0 justify-start gap-2 rounded-md pl-3 pr-8 py-2 text-[13px] font-light active:cursor-grabbing",
          isActive ? ACTIVE_CHAT_ROW_CLASS : INACTIVE_CHAT_ROW_CLASS,
        )}
      >
        {shouldReserveActivitySpace && (
          <span className="flex h-3 w-3 shrink-0 items-center justify-center">
            <SessionActivityIndicator
              isRunning={isRunning}
              hasUnread={hasUnread}
            />
          </span>
        )}
        <span className="flex-1 min-w-0 truncate text-left">
          {displayTitle}
        </span>
      </Button>

      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("menu.optionsFor", { label: displayTitle })}
            onClick={(e) => e.stopPropagation()}
            className={cn(
              "absolute right-1 size-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/50",
              menuOpen
                ? "visible opacity-100"
                : "invisible group-hover:visible opacity-0 group-hover:opacity-100",
            )}
          >
            <MoreHorizontal className="size-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" sideOffset={4}>
          <DropdownMenuItem onClick={startRename}>
            <Pencil className="size-3.5" />
            {t("common:actions.rename")}
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => {
              onArchive?.(id);
            }}
          >
            <Trash2 className="size-3.5" />
            {t("common:actions.archive")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
