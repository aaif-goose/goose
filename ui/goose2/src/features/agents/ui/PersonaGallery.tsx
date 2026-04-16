import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";
import type { Persona } from "@/shared/types/agents";
import { PersonaCard } from "@/features/agents/ui/PersonaCard";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";

interface PersonaGalleryProps {
  personas: Persona[];
  activePersonaId?: string;
  onSelectPersona: (persona: Persona) => void;
  onEditPersona: (persona: Persona) => void;
  onDuplicatePersona: (persona: Persona) => void;
  onDeletePersona: (persona: Persona) => void;
  onExportPersona?: (persona: Persona) => void;
  onCreatePersona: () => void;
  onImportFile?: (fileBytes: number[], fileName: string) => void;
  isLoading?: boolean;
}

function SkeletonCard() {
  return (
    <div
      aria-hidden="true"
      className="flex flex-col items-center gap-3 rounded-xl border border-border p-5"
    >
      <Skeleton className="h-12 w-12 rounded-full" />
      <Skeleton className="h-4 w-24" />
      <Skeleton className="h-3 w-full" />
      <Skeleton className="h-3 w-3/4" />
    </div>
  );
}

export function PersonaGallery({
  personas,
  activePersonaId,
  onSelectPersona,
  onEditPersona,
  onDuplicatePersona,
  onDeletePersona,
  onExportPersona,
  onCreatePersona,
  onImportFile,
  isLoading = false,
}: PersonaGalleryProps) {
  const { t } = useTranslation("agents");
  const { fileInputRef, isDragOver, dropHandlers, handleFileChange } =
    useFileImportZone({
      onImportFile: onImportFile ?? (() => {}),
    });
  const sorted = useMemo(() => {
    const builtins = personas
      .filter((p) => p.isBuiltin)
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
    const custom = personas
      .filter((p) => !p.isBuiltin)
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
    return [...builtins, ...custom];
  }, [personas]);

  if (isLoading) {
    return (
      <div
        role="status"
        aria-label={t("gallery.loading")}
        className="grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-4"
      >
        <SkeletonCard />
        <SkeletonCard />
        <SkeletonCard />
        <SkeletonCard />
      </div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-4">
      {sorted.map((persona) => (
        <PersonaCard
          key={persona.id}
          persona={persona}
          isActive={persona.id === activePersonaId}
          onSelect={onSelectPersona}
          onEdit={onEditPersona}
          onDuplicate={onDuplicatePersona}
          onDelete={onDeletePersona}
          onExport={onExportPersona}
        />
      ))}

      {/* Create new card */}
      <Button
        type="button"
        variant="ghost"
        onClick={onCreatePersona}
        aria-label={t("gallery.createAria")}
        {...dropHandlers}
        className={cn(
          "flex h-auto flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-5",
          "text-muted-foreground",
          "hover:border-border hover:text-muted-foreground hover:bg-accent/50",
          isDragOver
            ? "border-border bg-muted/50 text-muted-foreground"
            : "border-border",
        )}
      >
        <Plus className="size-8" />
        <span className="text-sm font-medium">{t("gallery.new")}</span>
        {onImportFile && (
          <span className="text-[11px] text-muted-foreground">
            {t("gallery.dropFile")}
          </span>
        )}
      </Button>
      {onImportFile && (
        <input
          ref={fileInputRef}
          type="file"
          accept=".persona.json,.json"
          className="hidden"
          onChange={handleFileChange}
        />
      )}
    </div>
  );
}
