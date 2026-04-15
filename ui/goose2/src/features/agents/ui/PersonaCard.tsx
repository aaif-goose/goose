import { useState } from "react";
import { useTranslation } from "react-i18next";
import { MoreVertical, Copy, Pencil, Trash2, Download } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Avatar, AvatarImage, AvatarFallback } from "@/shared/ui/avatar";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import type { Persona } from "@/shared/types/agents";

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

  const initials = persona.displayName.charAt(0).toUpperCase();
  const avatarSrc = useAvatarSrc(persona.avatar);

  return (
    <section
      aria-label={t("card.ariaLabel", { name: persona.displayName })}
      onClick={() => !menuOpen && onSelect?.(persona)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect?.(persona);
        }
      }}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: card needs keyboard focus but contains nested interactive buttons
      tabIndex={0}
      className={cn(
        "group relative flex flex-col items-center gap-3 rounded-xl border p-5 cursor-pointer",
        "bg-background transition-colors duration-200 motion-safe:animate-in motion-safe:fade-in motion-safe:slide-in-from-bottom-2",
        "hover:bg-accent/50",
        isActive ? "border-border ring-1 ring-ring" : "border-border",
      )}
    >
      {/* Dropdown trigger */}
      <div className="absolute right-2 top-2">
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={t("card.options")}
              onClick={(e) => e.stopPropagation()}
              className={cn(
                "size-6 rounded-md text-muted-foreground hover:text-foreground",
                menuOpen ? "opacity-100" : "opacity-0 group-hover:opacity-100",
              )}
            >
              <MoreVertical className="size-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" sideOffset={4}>
            <DropdownMenuItem onSelect={() => onEdit?.(persona)}>
              <Pencil className="size-3.5" />
              {t("common:actions.edit")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => onDuplicate?.(persona)}>
              <Copy className="size-3.5" />
              {t("common:actions.duplicate")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => onExport?.(persona)}>
              <Download className="size-3.5" />
              {t("common:actions.export")}
            </DropdownMenuItem>
            {!persona.isBuiltin && !persona.isFromDisk && (
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

      {/* Avatar */}
      <Avatar className="h-12 w-12">
        <AvatarImage src={avatarSrc ?? undefined} alt={persona.displayName} />
        <AvatarFallback className="text-sm font-semibold">
          {initials}
        </AvatarFallback>
      </Avatar>

      {/* Name */}
      <h3 className="text-sm font-medium text-center leading-tight">
        {persona.displayName}
      </h3>

      {/* Built-in badge */}
      {persona.isBuiltin && (
        <Badge variant="secondary" className="text-[10px]">
          {t("common:labels.builtIn")}
        </Badge>
      )}

      {/* System prompt preview */}
      <p className="text-xs text-muted-foreground text-center line-clamp-2 w-full">
        {persona.systemPrompt}
      </p>

      {/* Provider/model badge */}
      {(persona.provider || persona.model) && (
        <Badge variant="secondary" className="text-[10px]">
          {persona.provider && <span>{persona.provider}</span>}
          {persona.provider && persona.model && (
            <span aria-hidden="true">/</span>
          )}
          {persona.model && <span>{persona.model}</span>}
        </Badge>
      )}
    </section>
  );
}
