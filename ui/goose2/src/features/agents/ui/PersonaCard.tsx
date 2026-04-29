import { useState } from "react";
import { useTranslation } from "react-i18next";
import { MoreVertical, Copy, Pencil, Trash2, Download } from "lucide-react";
import { resolvePersonaFigure } from "@/features/agents/lib/personaFigure";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import type { Persona } from "@/shared/types/agents";
import { getPersonaSource } from "@/features/agents/lib/personaPresentation";

interface PersonaCardProps {
  persona: Persona;
  onSelect?: (persona: Persona) => void;
  onEdit?: (persona: Persona) => void;
  onDuplicate?: (persona: Persona) => void;
  onDelete?: (persona: Persona) => void;
  onExport?: (persona: Persona) => void;
  isActive?: boolean;
}

export function PersonaCard({
  persona,
  onSelect,
  onEdit,
  onDuplicate,
  onDelete,
  onExport,
  isActive = false,
}: PersonaCardProps) {
  const { t } = useTranslation(["agents", "common"]);
  const [menuOpen, setMenuOpen] = useState(false);

  const personaSource = getPersonaSource(persona);
  const canEditPersona = personaSource === "custom";
  const canDeletePersona = personaSource !== "builtin";

  const handleCardKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || menuOpen) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect?.(persona);
    }
  };

  return (
    // biome-ignore lint/a11y/useSemanticElements: card contains nested menu buttons, so a native button is not valid here
    <div
      aria-label={t("card.ariaLabel", { name: persona.displayName })}
      role="button"
      onClick={() => !menuOpen && onSelect?.(persona)}
      onKeyDown={handleCardKeyDown}
      tabIndex={0}
      className={cn(
        "group relative flex w-[160px] shrink-0 cursor-pointer flex-col py-4",
        "transition-colors duration-200",
        isActive && "bg-black/[0.03]",
      )}
    >
      {/* Fixed-height figure box: figure aligned to bottom, centered horizontally;
          wider figures bleed into the gap between cells (intentional editorial overflow) */}
      <div className="flex h-[336px] w-full items-end justify-center">
        <img
          src={resolvePersonaFigure(persona.displayName)}
          alt=""
          aria-hidden="true"
          className="h-full w-auto max-w-none select-none"
        />
      </div>

      <div className="mt-4 h-px w-full bg-black/30" />

      <div className="mt-4 w-full">
        <span className="inline-flex h-5 items-center rounded-full bg-[var(--surface-button)] px-[6px] pb-[3px] text-[14px] text-[var(--text-title-alex)]">
          {persona.displayName}
        </span>
      </div>

      <p className="mt-2 line-clamp-6 h-[144px] w-full text-[16px] leading-[24px] text-[var(--text-muted-alex)]">
        {persona.systemPrompt}
      </p>

      <div className="absolute top-2 right-2 opacity-0 transition-opacity duration-150 focus-within:opacity-100 group-hover:opacity-100">
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("card.options")}
              onClick={(e) => e.stopPropagation()}
              onKeyDown={(event) => event.stopPropagation()}
              className="size-6 rounded-md text-muted-foreground hover:text-foreground"
            >
              <MoreVertical className="size-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" sideOffset={4}>
            {canEditPersona && (
              <DropdownMenuItem onSelect={() => onEdit?.(persona)}>
                <Pencil className="size-3.5" />
                {t("common:actions.edit")}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onSelect={() => onDuplicate?.(persona)}>
              <Copy className="size-3.5" />
              {t("common:actions.duplicate")}
            </DropdownMenuItem>
            {onExport && (
              <DropdownMenuItem onSelect={() => onExport(persona)}>
                <Download className="size-3.5" />
                {t("common:actions.export")}
              </DropdownMenuItem>
            )}
            {canDeletePersona && (
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => onDelete?.(persona)}
              >
                <Trash2 className="size-3.5" />
                {t("common:actions.delete")}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
