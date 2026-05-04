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
  onStartChatPersona?: (persona: Persona) => void;
  onEditPersona: (persona: Persona) => void;
  onDuplicatePersona: (persona: Persona) => void;
  onDeletePersona: (persona: Persona) => void;
  onCopyPersonaFile?: (persona: Persona) => void;
  onSavePersonaCopy?: (persona: Persona) => void;
  onCreatePersona: () => void;
  onImportFile?: (fileBytes: number[], fileName: string) => void;
  validateImportFile?: (file: Pick<File, "name" | "type">) => string | null;
  onImportError?: (message: string) => void;
  isLoading?: boolean;
  hasAnyPersonas?: boolean;
}

function SkeletonCard() {
  return (
    <div
      aria-hidden="true"
      className="flex flex-col rounded-2xl border border-border-soft bg-background p-5"
    >
      <div className="flex items-start justify-between gap-3">
        <Skeleton className="h-12 w-12 rounded-full" />
        <Skeleton className="h-6 w-6 rounded-md" />
      </div>
      <div className="mt-5 min-w-0 space-y-3">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-5/6" />
      </div>
      <div aria-hidden="true" className="h-7 shrink-0" />
      <div>
        <Skeleton className="h-3 w-3/4" />
      </div>
    </div>
  );
}

export function PersonaGallery({
  personas,
  activePersonaId,
  onSelectPersona,
  onStartChatPersona,
  onEditPersona,
  onDuplicatePersona,
  onDeletePersona,
  onCopyPersonaFile,
  onSavePersonaCopy,
  onCreatePersona,
  onImportFile,
  validateImportFile,
  onImportError,
  isLoading = false,
  hasAnyPersonas = personas.length > 0,
}: PersonaGalleryProps) {
  const { t } = useTranslation("agents");
  const { fileInputRef, isDragOver, dropHandlers, handleFileChange } =
    useFileImportZone({
      onImportFile: onImportFile ?? (() => {}),
      validateFile: validateImportFile,
      onImportError,
    });
  const sortedPersonas = useMemo(
    () =>
      [...personas].sort((a, b) => a.displayName.localeCompare(b.displayName)),
    [personas],
  );

  if (isLoading) {
    return (
      <div
        role="status"
        aria-label={t("gallery.loading")}
        className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3"
      >
        <SkeletonCard />
        <SkeletonCard />
        <SkeletonCard />
        <SkeletonCard />
      </div>
    );
  }

  if (personas.length === 0) {
    return (
      <div
        {...dropHandlers}
        className={cn(
          "flex min-h-72 flex-col items-center justify-center rounded-2xl border border-dashed border-border-soft bg-muted/10 px-6 text-center",
          isDragOver && "border-border bg-muted/30",
        )}
      >
        <p className="text-sm font-medium text-foreground">
          {hasAnyPersonas ? t("gallery.noResults") : t("view.emptyAgentsTitle")}
        </p>
        <p className="mt-1 max-w-sm text-xs leading-5 text-muted-foreground">
          {hasAnyPersonas
            ? t("gallery.noResultsDescription")
            : t("view.emptyAgentsDescription")}
        </p>
        <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
          <Button type="button" size="sm" onClick={onCreatePersona}>
            <Plus className="size-3.5" />
            {t("gallery.new")}
          </Button>
        </div>
        {onImportFile && (
          <>
            <p className="mt-3 text-[11px] text-muted-foreground">
              {t("gallery.dropFile")}
            </p>
            <input
              ref={fileInputRef}
              type="file"
              accept=".md,text/markdown,text/plain"
              className="hidden"
              onChange={handleFileChange}
            />
          </>
        )}
      </div>
    );
  }

  return (
    <div className="grid auto-rows-fr grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
      {sortedPersonas.map((persona) => (
        <PersonaCard
          key={persona.id}
          persona={persona}
          isActive={persona.id === activePersonaId}
          onSelect={onSelectPersona}
          onStartChat={onStartChatPersona}
          onEdit={onEditPersona}
          onDuplicate={onDuplicatePersona}
          onDelete={onDeletePersona}
          onCopyFile={onCopyPersonaFile}
          onSaveCopy={onSavePersonaCopy}
        />
      ))}

      <Button
        type="button"
        variant="ghost"
        onClick={onCreatePersona}
        aria-label={t("gallery.createAria")}
        {...dropHandlers}
        className={cn(
          "flex h-full min-h-48 w-full flex-col items-center justify-center gap-2 rounded-2xl border border-dashed p-5",
          "text-muted-foreground transition-colors",
          "hover:border-border hover:text-foreground hover:bg-muted/20",
          isDragOver
            ? "border-border bg-muted/50 text-muted-foreground"
            : "border-border-soft",
        )}
      >
        <Plus className="size-6" />
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
          accept=".md,text/markdown,text/plain"
          className="hidden"
          onChange={handleFileChange}
        />
      )}
    </div>
  );
}
