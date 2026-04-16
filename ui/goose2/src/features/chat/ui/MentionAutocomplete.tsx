import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Sparkles, User } from "lucide-react";
import { IconFile, IconFolder } from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import { PopoverContent } from "@/shared/ui/popover";
import type { Persona } from "@/shared/types/agents";

// ---------------------------------------------------------------------------
// Fuzzy subsequence matcher (fzf-style)
// ---------------------------------------------------------------------------

/** Returns true when every character in `query` appears in `target` in order. */
export function fuzzyMatch(query: string, target: string): boolean {
  let qi = 0;
  for (let ti = 0; ti < target.length && qi < query.length; ti++) {
    if (query[qi] === target[ti]) qi++;
  }
  return qi === query.length;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FileMentionItem {
  /** Absolute path used when inserting into the message. */
  resolvedPath: string;
  /** Shortened display path (e.g. ~/project/src/foo.ts). */
  displayPath: string;
  /** Just the filename portion. */
  filename: string;
  kind: "file" | "folder" | "path";
}

export type MentionItem =
  | { type: "persona"; persona: Persona }
  | { type: "file"; file: FileMentionItem };

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface MentionAutocompleteProps {
  /** Pre-filtered personas from the hook. */
  filteredPersonas: Persona[];
  /** Pre-filtered files from the hook. */
  filteredFiles?: FileMentionItem[];
  isOpen: boolean;
  onSelectPersona: (persona: Persona) => void;
  onSelectFile?: (file: FileMentionItem) => void;
  onClose?: (() => void) | undefined;
  selectedIndex?: number;
}

export function MentionAutocomplete({
  filteredPersonas,
  filteredFiles = [],
  isOpen,
  onSelectPersona,
  onSelectFile,
  selectedIndex: controlledIndex,
}: MentionAutocompleteProps) {
  const { t } = useTranslation("chat");
  const [internalIndex, setInternalIndex] = useState(0);
  const selectedIndex = controlledIndex ?? internalIndex;
  const itemRefs = useRef<Map<number, HTMLButtonElement>>(new Map());

  // Scroll the active item into view when selectedIndex changes
  useEffect(() => {
    const el = itemRefs.current.get(selectedIndex);
    if (el) {
      el.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  const items: MentionItem[] = useMemo(() => {
    const result: MentionItem[] = filteredPersonas.map((p) => ({
      type: "persona" as const,
      persona: p,
    }));
    for (const f of filteredFiles) {
      result.push({ type: "file" as const, file: f });
    }
    return result;
  }, [filteredPersonas, filteredFiles]);

  const handleSelect = useCallback(
    (item: MentionItem) => {
      if (item.type === "persona") {
        onSelectPersona(item.persona);
      } else {
        onSelectFile?.(item.file);
      }
    },
    [onSelectPersona, onSelectFile],
  );

  if (!isOpen || items.length === 0) return null;

  return (
    <PopoverContent
      side="top"
      align="start"
      sideOffset={4}
      className="w-72 px-1 py-1"
      onOpenAutoFocus={(e) => e.preventDefault()}
      onCloseAutoFocus={(e) => e.preventDefault()}
      onEscapeKeyDown={(e) => e.preventDefault()}
      onInteractOutside={(e) => e.preventDefault()}
      role="listbox"
      aria-label={t("mention.ariaLabel")}
    >
      <div className="max-h-56 overflow-y-auto">
        {filteredPersonas.length > 0 && (
          <div className="px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            {t("mention.title")}
          </div>
        )}
        {filteredPersonas.map((persona, index) => (
          <button
            key={persona.id}
            ref={(el) => {
              if (el) itemRefs.current.set(index, el);
              else itemRefs.current.delete(index);
            }}
            type="button"
            role="option"
            aria-selected={index === selectedIndex}
            className={cn(
              "flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left transition-colors",
              index === selectedIndex
                ? "bg-accent text-foreground"
                : "text-muted-foreground hover:bg-accent/50",
            )}
            onClick={() => handleSelect({ type: "persona", persona })}
            onMouseEnter={() => setInternalIndex(index)}
          >
            <MentionAvatar persona={persona} />
            <div className="flex min-w-0 flex-col">
              <span className="text-sm font-medium">{persona.displayName}</span>
              {persona.provider && (
                <span className="text-[10px] text-muted-foreground">
                  {persona.provider}
                  {persona.model
                    ? ` / ${persona.model.split("-").slice(0, 2).join("-")}`
                    : ""}
                </span>
              )}
            </div>
          </button>
        ))}

        {filteredFiles.length > 0 && (
          <div className="mt-1 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            {t("mention.filesTitle")}
          </div>
        )}
        {filteredFiles.map((file, i) => {
          const globalIndex = filteredPersonas.length + i;
          return (
            <button
              key={file.resolvedPath}
              ref={(el) => {
                if (el) itemRefs.current.set(globalIndex, el);
                else itemRefs.current.delete(globalIndex);
              }}
              type="button"
              role="option"
              aria-selected={globalIndex === selectedIndex}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors",
                globalIndex === selectedIndex
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-accent/50",
              )}
              onClick={() => handleSelect({ type: "file", file })}
              onMouseEnter={() => setInternalIndex(globalIndex)}
            >
              {file.kind === "folder" ? (
                <IconFolder className="size-4 shrink-0" />
              ) : (
                <IconFile className="size-4 shrink-0" />
              )}
              <div className="flex min-w-0 flex-col">
                <span className="truncate text-sm font-medium">
                  {file.filename}
                </span>
                <span className="truncate text-[10px] text-muted-foreground">
                  {file.displayPath}
                </span>
              </div>
            </button>
          );
        })}
      </div>
    </PopoverContent>
  );
}

// ---------------------------------------------------------------------------
// Avatar helper
// ---------------------------------------------------------------------------

function MentionAvatar({ persona }: { persona: Persona }) {
  const avatarSrc = useAvatarSrc(persona.avatar);
  if (avatarSrc) {
    return (
      <img
        src={avatarSrc}
        alt={persona.displayName}
        className="h-7 w-7 rounded-full object-cover"
      />
    );
  }

  return (
    <div
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-full",
        persona.isBuiltin
          ? "bg-foreground/10 text-foreground"
          : "bg-brand/10 text-brand",
      )}
    >
      {persona.isBuiltin ? (
        <Sparkles className="h-3.5 w-3.5" />
      ) : (
        <User className="h-3.5 w-3.5" />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Hook — mention detection + keyboard navigation
// ---------------------------------------------------------------------------

export function useMentionDetection(
  personas: Persona[] = [],
  files: FileMentionItem[] = [],
) {
  const [mentionState, setMentionState] = useState<{
    isOpen: boolean;
    query: string;
    startIndex: number;
    selectedIndex: number;
  }>({ isOpen: false, query: "", startIndex: -1, selectedIndex: 0 });

  const { filteredPersonas, filteredFiles } = useMemo(() => {
    if (!mentionState.isOpen) {
      return { filteredPersonas: personas, filteredFiles: files };
    }
    const q = mentionState.query.toLowerCase();
    if (!q) return { filteredPersonas: personas, filteredFiles: files };
    return {
      filteredPersonas: personas.filter((p) =>
        fuzzyMatch(q, p.displayName.toLowerCase()),
      ),
      filteredFiles: files.filter(
        (f) =>
          fuzzyMatch(q, f.filename.toLowerCase()) ||
          fuzzyMatch(q, f.displayPath.toLowerCase()),
      ),
    };
  }, [personas, files, mentionState.isOpen, mentionState.query]);

  const totalCount = filteredPersonas.length + filteredFiles.length;

  const detectMention = useCallback(
    (value: string, cursorPos: number) => {
      const beforeCursor = value.slice(0, cursorPos);
      const lastAt = beforeCursor.lastIndexOf("@");

      if (lastAt === -1) {
        if (mentionState.isOpen) {
          setMentionState({
            isOpen: false,
            query: "",
            startIndex: -1,
            selectedIndex: 0,
          });
        }
        return;
      }

      // @ must be at start of input or preceded by whitespace
      if (lastAt > 0 && !/\s/.test(beforeCursor[lastAt - 1])) {
        if (mentionState.isOpen) {
          setMentionState({
            isOpen: false,
            query: "",
            startIndex: -1,
            selectedIndex: 0,
          });
        }
        return;
      }

      const query = beforeCursor.slice(lastAt + 1);

      // Close if there's a space after the query (mention completed) or too long
      if (query.includes(" ") || query.length > 50) {
        if (mentionState.isOpen) {
          setMentionState({
            isOpen: false,
            query: "",
            startIndex: -1,
            selectedIndex: 0,
          });
        }
        return;
      }

      setMentionState((prev) => ({
        isOpen: true,
        query,
        startIndex: lastAt,
        selectedIndex: prev.query !== query ? 0 : prev.selectedIndex,
      }));
    },
    [mentionState.isOpen],
  );

  const closeMention = useCallback(() => {
    setMentionState({
      isOpen: false,
      query: "",
      startIndex: -1,
      selectedIndex: 0,
    });
  }, []);

  const navigateMention = useCallback(
    (direction: "up" | "down"): boolean => {
      if (!mentionState.isOpen || totalCount === 0) return false;
      setMentionState((prev) => {
        const delta = direction === "down" ? 1 : -1;
        const next = (prev.selectedIndex + delta + totalCount) % totalCount;
        return { ...prev, selectedIndex: next };
      });
      return true;
    },
    [mentionState.isOpen, totalCount],
  );

  /** Confirm the currently highlighted item. Returns persona, file, or null. */
  const confirmMention = useCallback((): MentionItem | null => {
    if (!mentionState.isOpen || totalCount === 0) return null;
    const idx = mentionState.selectedIndex;
    if (idx < filteredPersonas.length) {
      return { type: "persona", persona: filteredPersonas[idx] };
    }
    const fileIdx = idx - filteredPersonas.length;
    if (fileIdx < filteredFiles.length) {
      return { type: "file", file: filteredFiles[fileIdx] };
    }
    return null;
  }, [
    mentionState.isOpen,
    mentionState.selectedIndex,
    totalCount,
    filteredPersonas,
    filteredFiles,
  ]);

  return {
    mentionOpen: mentionState.isOpen,
    mentionQuery: mentionState.query,
    mentionStartIndex: mentionState.startIndex,
    mentionSelectedIndex: mentionState.selectedIndex,
    filteredPersonas,
    filteredFiles,
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
  };
}
