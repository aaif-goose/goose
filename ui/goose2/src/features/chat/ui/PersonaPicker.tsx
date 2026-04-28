import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AtSign, ChevronDown, Check, Plus, Sparkles, User } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/shared/ui/dropdown-menu";
import { Button } from "@/shared/ui/button";
import type { Persona } from "@/shared/types/agents";

interface PersonaPickerProps {
  personas: Persona[];
  selectedPersonaId: string | null;
  onPersonaChange: (personaId: string | null) => void;
  onCreatePersona?: () => void;
  compact?: boolean;
  className?: string;
  triggerVariant?: "default" | "icon";
}

export function PersonaPicker({
  personas,
  selectedPersonaId,
  onPersonaChange,
  onCreatePersona,
  compact = false,
  className,
  triggerVariant = "default",
}: PersonaPickerProps) {
  const { t } = useTranslation(["chat", "common"]);
  const selected = useMemo(
    () =>
      selectedPersonaId
        ? personas.find((p) => p.id === selectedPersonaId)
        : undefined,
    [personas, selectedPersonaId],
  );

  const builtinPersonas = useMemo(
    () => personas.filter((p) => p.isBuiltin),
    [personas],
  );
  const customPersonas = useMemo(
    () => personas.filter((p) => !p.isBuiltin),
    [personas],
  );

  const label = selected?.displayName ?? t("common:labels.goose");

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        {triggerVariant === "icon" ? (
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className={className}
            aria-label={t("persona.chooseAssistant")}
            title={t("persona.chooseAssistant")}
          >
            <AtSign />
          </Button>
        ) : (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className={cn(
              "gap-1.5 rounded-lg px-2.5 font-medium text-foreground hover:bg-accent",
              className,
            )}
            aria-label={t("persona.chooseAssistant")}
          >
            <PersonaAvatar persona={selected} size="sm" />
            {!compact && <span>{label}</span>}
            <ChevronDown className="h-3 w-3 opacity-50" />
          </Button>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className="max-h-[min(70vh,32rem)] w-[22rem] overflow-y-auto"
      >
        <DropdownMenuItem
          onSelect={() => onPersonaChange(null)}
          className="flex items-start gap-2.5 py-2"
        >
          <div className="flex h-6 w-6 items-center justify-center rounded-full bg-foreground/10 text-foreground">
            <Sparkles className="h-3.5 w-3.5" />
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-sm font-medium">
              {t("common:labels.goose")}
            </span>
            <span className="text-[11px] leading-snug text-muted-foreground">
              {t("persona.defaultDescription")}
            </span>
          </div>
          {selectedPersonaId === null && (
            <Check className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          )}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        {builtinPersonas.length > 0 && (
          <>
            <DropdownMenuLabel className="text-[11px] uppercase tracking-wider text-muted-foreground">
              {t("common:labels.builtIn")}
            </DropdownMenuLabel>
            {builtinPersonas.map((persona) => (
              <PersonaMenuItem
                key={persona.id}
                persona={persona}
                isSelected={persona.id === selectedPersonaId}
                onSelect={() => onPersonaChange(persona.id)}
              />
            ))}
          </>
        )}
        {customPersonas.length > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-[11px] uppercase tracking-wider text-muted-foreground">
              {t("common:labels.custom")}
            </DropdownMenuLabel>
            {customPersonas.map((persona) => (
              <PersonaMenuItem
                key={persona.id}
                persona={persona}
                isSelected={persona.id === selectedPersonaId}
                onSelect={() => onPersonaChange(persona.id)}
              />
            ))}
          </>
        )}
        {onCreatePersona && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={onCreatePersona}>
              <Plus className="mr-2 h-3.5 w-3.5 text-muted-foreground" />
              <span className="text-sm">{t("persona.create")}</span>
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function PersonaMenuItem({
  persona,
  isSelected,
  onSelect,
}: {
  persona: Persona;
  isSelected: boolean;
  onSelect: () => void;
}) {
  // Keep the first sentence as a summary and let layout, not string slicing,
  // decide where the visible truncation happens.
  const summary = useMemo(() => {
    return persona.systemPrompt.split(/\.\s/)[0]?.trim() ?? "";
  }, [persona.systemPrompt]);

  return (
    <DropdownMenuItem
      onSelect={onSelect}
      className="flex items-start gap-2.5 py-2"
    >
      <PersonaAvatar persona={persona} size="md" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm font-medium">{persona.displayName}</span>
        {summary && (
          <p className="line-clamp-2 break-words text-[11px] leading-snug text-muted-foreground">
            {summary}
          </p>
        )}
      </div>
      {isSelected && (
        <Check className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
      )}
    </DropdownMenuItem>
  );
}

function PersonaAvatar({
  persona,
  size = "sm",
}: {
  persona?: Persona;
  size?: "xs" | "sm" | "md";
}) {
  const dim =
    size === "xs" ? "h-3.5 w-3.5" : size === "sm" ? "h-4 w-4" : "h-6 w-6";
  const iconDim = size === "md" ? "h-3.5 w-3.5" : "h-2.5 w-2.5";

  const avatarSrc = useAvatarSrc(persona?.avatar);
  if (avatarSrc) {
    return (
      <img
        src={avatarSrc}
        alt={persona?.displayName ?? ""}
        className={cn(dim, "rounded-full object-cover")}
      />
    );
  }

  const isBuiltin = persona?.isBuiltin ?? true;

  return (
    <div
      className={cn(
        dim,
        "flex items-center justify-center rounded-full",
        isBuiltin
          ? "bg-foreground/10 text-foreground"
          : "bg-brand/10 text-brand",
      )}
    >
      {isBuiltin ? (
        <Sparkles className={iconDim} />
      ) : (
        <User className={iconDim} />
      )}
    </div>
  );
}

export { PersonaAvatar };
