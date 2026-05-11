import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Sparkles, User } from "lucide-react";
import { IconFile, IconFolder } from "@tabler/icons-react";
import { SkillIcon } from "@/features/skills/ui/SkillIcon";
import { cn } from "@/shared/lib/cn";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import { PopoverContent } from "@/shared/ui/popover";
import type { Persona } from "@/shared/types/agents";
import type {
  FileMentionItem,
  MentionItem,
  SkillMentionItem,
} from "./mentionDetection";
export { fuzzyMatch, useMentionDetection } from "./mentionDetection";
export type {
  FileMentionItem,
  MentionItem,
  SkillMentionItem,
} from "./mentionDetection";

interface MentionAutocompleteProps {
  /** Pre-filtered personas from the hook. */
  filteredPersonas: Persona[];
  /** Pre-filtered skills from the hook. */
  filteredSkills?: SkillMentionItem[];
  /** Pre-filtered files from the hook. */
  filteredFiles?: FileMentionItem[];
  isOpen: boolean;
  onSelectPersona: (persona: Persona) => void;
  onSelectSkill?: (skill: SkillMentionItem) => void;
  onSelectFile?: (file: FileMentionItem) => void;
  onClose?: (() => void) | undefined;
  selectedIndex?: number;
}

export function MentionAutocomplete({
  filteredPersonas,
  filteredSkills = [],
  filteredFiles = [],
  isOpen,
  onSelectPersona,
  onSelectSkill,
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
    for (const skill of filteredSkills) {
      result.push({ type: "skill" as const, skill });
    }
    for (const f of filteredFiles) {
      result.push({ type: "file" as const, file: f });
    }
    return result;
  }, [filteredPersonas, filteredSkills, filteredFiles]);

  const handleSelect = useCallback(
    (item: MentionItem) => {
      if (item.type === "persona") {
        onSelectPersona(item.persona);
      } else if (item.type === "skill") {
        onSelectSkill?.(item.skill);
      } else {
        onSelectFile?.(item.file);
      }
    },
    [onSelectPersona, onSelectSkill, onSelectFile],
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

        {filteredSkills.length > 0 && (
          <div className="mt-1 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            {t("mention.skillsTitle")}
          </div>
        )}
        {filteredSkills.map((skill, i) => {
          const globalIndex = filteredPersonas.length + i;
          return (
            <button
              key={skill.id}
              ref={(el) => {
                if (el) itemRefs.current.set(globalIndex, el);
                else itemRefs.current.delete(globalIndex);
              }}
              type="button"
              role="option"
              aria-selected={globalIndex === selectedIndex}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left transition-colors",
                globalIndex === selectedIndex
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-accent/50",
              )}
              onClick={() => handleSelect({ type: "skill", skill })}
              onMouseEnter={() => setInternalIndex(globalIndex)}
            >
              <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-brand/10 text-brand">
                <SkillIcon className="h-3.5 w-3.5" />
              </div>
              <div className="flex min-w-0 flex-col">
                <span className="text-sm font-medium">{skill.name}</span>
                <span className="truncate text-[10px] text-muted-foreground">
                  {skill.description || skill.sourceLabel}
                </span>
              </div>
            </button>
          );
        })}

        {filteredFiles.length > 0 && (
          <div className="mt-1 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            {t("mention.filesTitle")}
          </div>
        )}
        {filteredFiles.map((file, i) => {
          const globalIndex =
            filteredPersonas.length + filteredSkills.length + i;
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
