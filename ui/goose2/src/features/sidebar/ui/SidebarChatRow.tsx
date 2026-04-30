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
  "text-foreground hover:bg-background-alt hover:text-foreground";
const ACTIVE_CHAT_ROW_CLASS =
  "bg-background-alt font-normal text-foreground hover:bg-background-alt hover:text-foreground";

interface SidebarChatRowProps {
  id: string;
  title: string;
  isActive: boolean;
  isRunning?: boolean;
  hasUnread?: boolean;
  className?: string;
  nested?: boolean;
  onSelect?: (id: string) => void;
  onRename?: (id: string, nextTitle: string) => void;
  onArchive?: (id: string) => void;
}

export function SidebarChatRow({
  id,
  title,
  isActive,
  isRunning = false,
  hasUnread = false,
  className,
  nested = false,
  onSelect,
  onRename,
  onArchive,
}: SidebarChatRowProps) {
  const { t } = useTranslation(["sidebar", "common"]);
  const [menuOpen, setMenuOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
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

  useEffect(() => {
    setDraftTitle(editableTitle);
  }, [editableTitle]);

  useEffect(() => {
    if (!editing) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [editing]);

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
          className="flex-1 min-w-0 px-3 text-sm font-normal"
          style={{ height: 32 }}
        />
      </div>
    );
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: wrapper handles drag and context menu, interactive content is the inner Button
    <div
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
        "relative flex items-center group/chat-row rounded-md transition-colors duration-200 hover:bg-background-alt focus-within:bg-background-alt active:cursor-grabbing",
        (isActive || menuOpen) && "bg-background-alt",
        dragging && "opacity-40 bg-accent/30",
        className,
      )}
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
          "flex-1 min-w-0 justify-start gap-2 rounded-md pr-8 py-2 text-sm font-normal active:cursor-grabbing",
          nested ? "pl-9" : "pl-3",
          isActive ? ACTIVE_CHAT_ROW_CLASS : INACTIVE_CHAT_ROW_CLASS,
        )}
      >
        {showActivityIndicator && !nested && (
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
      {showActivityIndicator && nested && (
        <SessionActivityIndicator
          isRunning={isRunning}
          hasUnread={hasUnread}
          variant="overlay"
          className="left-4 right-auto top-1/2 -translate-y-1/2"
        />
      )}

      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("menu.optionsFor", { label: displayTitle })}
            onClick={(e) => e.stopPropagation()}
            className={cn(
              "absolute right-1 size-6 rounded-md",
              menuOpen
                ? "visible opacity-100"
                : "invisible group-hover/chat-row:visible opacity-0 group-hover/chat-row:opacity-100",
            )}
          >
            <MoreHorizontal className="size-4" />
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
