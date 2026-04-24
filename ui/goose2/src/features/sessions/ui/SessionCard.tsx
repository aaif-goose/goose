import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Calendar,
  Folder,
  Bot,
  MoreHorizontal,
  Pencil,
  Trash2,
  ArchiveRestore,
  Copy,
  Download,
} from "lucide-react";
import {
  getDisplaySessionTitle,
  getEditableSessionTitle,
  isSessionTitleUnchanged,
} from "@/features/chat/lib/sessionTitle";
import { useLocaleFormatting } from "@/shared/i18n";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Input } from "@/shared/ui/input";

interface SessionCardProps {
  id: string;
  title: string;
  updatedAt: string;
  personaName?: string;
  projectName?: string;
  projectColor?: string;
  workingDir?: string;
  archivedAt?: string;
  snippet?: string;
  matchCount?: number;
  onSelect?: (id: string) => void;
  onRename?: (id: string, nextTitle: string) => void;
  onArchive?: (id: string) => void;
  onUnarchive?: (id: string) => void;
  onExport?: (id: string) => void;
  onDuplicate?: (id: string) => void;
}

export function SessionCard({
  id,
  title,
  updatedAt,
  personaName,
  projectName,
  projectColor,
  workingDir,
  archivedAt,
  snippet,
  matchCount,
  onSelect,
  onRename,
  onArchive,
  onUnarchive,
  onExport,
  onDuplicate,
}: SessionCardProps) {
  const { t } = useTranslation(["sessions", "common"]);
  const { formatRelativeTimeToNow } = useLocaleFormatting();
  const [menuOpen, setMenuOpen] = useState(false);
  const [editing, setEditing] = useState(false);
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

  const cancelRename = () => {
    setDraftTitle(editableTitle);
    setEditing(false);
  };

  return (
    <div
      className={cn(
        "group relative flex flex-col gap-2 rounded-lg border border-border bg-background p-4 text-left transition-shadow hover:shadow-card",
        archivedAt && "opacity-60",
      )}
    >
      {/* Click-to-open overlay */}
      <button
        type="button"
        onClick={() => onSelect?.(id)}
        className="absolute inset-0 z-0 rounded-lg"
        aria-label={t("card.open", { title: displayTitle })}
      />

      {/* Title — editable or static */}
      {editing ? (
        <Input
          ref={inputRef}
          type="text"
          value={draftTitle}
          onChange={(e) => setDraftTitle(e.target.value)}
          onBlur={commitRename}
          onClick={(e) => e.stopPropagation()}
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
          className="relative z-10 text-sm font-medium"
        />
      ) : (
        <p className="text-sm font-medium line-clamp-2 break-words pr-6">
          {displayTitle}
        </p>
      )}

      <div className="flex flex-col gap-1 text-xs text-muted-foreground">
        <div className="flex items-center gap-1.5">
          <Calendar className="size-3 shrink-0" />
          <span>{formatRelativeTimeToNow(updatedAt)}</span>
        </div>

        {personaName && (
          <div className="flex items-center gap-1.5">
            <Bot className="size-3 shrink-0" />
            <span className="truncate">{personaName}</span>
          </div>
        )}

        {projectName && (
          <div className="flex items-center gap-1.5">
            <span
              className="inline-block size-2 shrink-0 rounded-full"
              style={
                projectColor ? { backgroundColor: projectColor } : undefined
              }
            />
            <span className="truncate">{projectName}</span>
          </div>
        )}

        {workingDir && (
          <div className="flex items-center gap-1.5">
            <Folder className="size-3 shrink-0" />
            <span className="truncate">{workingDir}</span>
          </div>
        )}
      </div>

      {(snippet || matchCount) && (
        <div className="relative z-10 mt-1 space-y-1 text-xs">
          {snippet && (
            <p className="line-clamp-3 text-muted-foreground">{snippet}</p>
          )}
          {typeof matchCount === "number" && (
            <p className="font-medium text-foreground/80">
              {t("search.messageMatches", {
                count: matchCount,
                displayCount: matchCount,
              })}
            </p>
          )}
        </div>
      )}

      {/* Actions menu */}
      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("card.optionsFor", { title: displayTitle })}
            onClick={(e) => e.stopPropagation()}
            className={cn(
              "absolute right-2 top-2 z-10 size-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent/50",
              menuOpen
                ? "visible opacity-100"
                : "invisible group-hover:visible opacity-0 group-hover:opacity-100",
            )}
          >
            <MoreHorizontal className="size-3.5" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" sideOffset={4}>
          {archivedAt ? (
            <>
              <DropdownMenuItem
                onClick={() => {
                  setMenuOpen(false);
                  onExport?.(id);
                }}
              >
                <Download className="size-3.5" />
                {t("common:actions.export")}
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setMenuOpen(false);
                  onUnarchive?.(id);
                }}
              >
                <ArchiveRestore className="size-3.5" />
                {t("common:actions.restore")}
              </DropdownMenuItem>
            </>
          ) : (
            <>
              <DropdownMenuItem onClick={startRename}>
                <Pencil className="size-3.5" />
                {t("common:actions.rename")}
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setMenuOpen(false);
                  onExport?.(id);
                }}
              >
                <Download className="size-3.5" />
                {t("common:actions.export")}
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setMenuOpen(false);
                  onDuplicate?.(id);
                }}
              >
                <Copy className="size-3.5" />
                {t("common:actions.duplicate")}
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setMenuOpen(false);
                  onArchive?.(id);
                }}
              >
                <Trash2 className="size-3.5" />
                {t("common:actions.archive")}
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
