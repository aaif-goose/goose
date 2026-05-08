import { useState, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Upload } from "lucide-react";
import { toast } from "sonner";
import { downloadBase64File } from "@/shared/lib/encoding";
import { SearchBar } from "@/shared/ui/SearchBar";
import { Button, buttonVariants } from "@/shared/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import {
  selectPersonas,
  selectPersonasLoading,
} from "@/features/agents/stores/agentSelectors";
import { PersonaGallery } from "@/features/agents/ui/PersonaGallery";
import { PersonaEditor } from "@/features/agents/ui/PersonaEditor";
import { exportPersona, importPersonas } from "@/shared/api/agents";
import { usePersonas } from "@/features/agents/hooks/usePersonas";
import type {
  Persona,
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";
import {
  formatAgentError,
  formatImportSuccessMessage,
} from "@/features/agents/lib/personaImport";
import { getPersonaSource } from "@/features/agents/lib/personaPresentation";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";

export function AgentsView() {
  const { t } = useTranslation(["agents", "common"]);
  const [search, setSearch] = useState("");
  const [deletingPersona, setDeletingPersona] = useState<Persona | null>(null);

  const personas = useAgentStore(selectPersonas);
  const personasLoading = useAgentStore(selectPersonasLoading);
  const personaEditorOpen = useAgentStore((s) => s.personaEditorOpen);
  const editingPersona = useAgentStore((s) => s.editingPersona);
  const personaEditorMode = useAgentStore((s) => s.personaEditorMode);
  const openPersonaEditor = useAgentStore((s) => s.openPersonaEditor);
  const closePersonaEditor = useAgentStore((s) => s.closePersonaEditor);

  const {
    createPersona,
    updatePersona: updatePersonaViaHook,
    deletePersona,
    refreshFromDisk,
  } = usePersonas();

  const lowerSearch = search.toLowerCase();

  const filteredPersonas = useMemo(
    () =>
      personas.filter(
        (p) =>
          p.displayName.toLowerCase().includes(lowerSearch) ||
          p.systemPrompt.toLowerCase().includes(lowerSearch),
      ),
    [personas, lowerSearch],
  );

  const handleSavePersona = useCallback(
    async (data: CreatePersonaRequest | UpdatePersonaRequest) => {
      try {
        if (editingPersona && personaEditorMode === "edit") {
          await updatePersonaViaHook(
            editingPersona.id,
            data as UpdatePersonaRequest,
          );
          toast.success(t("editor.updated"));
        } else {
          await createPersona(data as CreatePersonaRequest);
          toast.success(t("editor.created"));
        }
        closePersonaEditor();
      } catch (error) {
        toast.error(formatAgentError(error, t("editor.saveFailed")));
      }
    },
    [
      closePersonaEditor,
      createPersona,
      editingPersona,
      personaEditorMode,
      t,
      updatePersonaViaHook,
    ],
  );

  const handleDuplicatePersona = useCallback(
    async (persona: Persona) => {
      try {
        await createPersona({
          displayName: t("view.copyName", { name: persona.displayName }),
          avatar: persona.avatar ?? undefined,
          systemPrompt: persona.systemPrompt,
          provider: persona.provider,
          model: persona.model,
        });
        toast.success(t("editor.duplicated"));
      } catch (error) {
        toast.error(formatAgentError(error, t("editor.saveFailed")));
      }
    },
    [createPersona, t],
  );

  const handleDeletePersona = useCallback((persona: Persona) => {
    if (getPersonaSource(persona) === "builtin" || persona.writable === false)
      return;
    setDeletingPersona(persona);
  }, []);

  const handleConfirmDeletePersona = useCallback(async () => {
    if (!deletingPersona) return;
    try {
      await deletePersona(deletingPersona.id);
      if (editingPersona?.id === deletingPersona.id) {
        closePersonaEditor();
      }
      toast.success(t("view.deleted", { name: deletingPersona.displayName }));
    } catch (err) {
      toast.error(formatAgentError(err, t("view.deleteFailed")));
    }
    setDeletingPersona(null);
  }, [closePersonaEditor, deletingPersona, deletePersona, editingPersona, t]);

  const handleExportPersona = useCallback(
    async (persona: Persona) => {
      try {
        const result = await exportPersona(persona.id);
        downloadBase64File(
          result.data,
          result.suggestedFilename,
          result.mimeType,
        );
        toast.success(
          t("view.exportedTo", { filename: result.suggestedFilename }),
        );
      } catch (err) {
        toast.error(formatAgentError(err, t("view.exportFailed")));
      }
    },
    [t],
  );

  const handleImportError = useCallback((message: string) => {
    toast.error(message);
  }, []);

  const handleImportFileBytes = useCallback(
    async (fileBytes: number[], fileName: string) => {
      try {
        const imported = await importPersonas(fileBytes, fileName);
        await refreshFromDisk();
        const message = formatImportSuccessMessage(imported.length);
        toast.success(t(message.key, message.options));
      } catch (err) {
        toast.error(formatAgentError(err, t("view.importFailed")));
      }
    },
    [refreshFromDisk, t],
  );

  const { fileInputRef, handleFileChange, openFilePicker } = useFileImportZone({
    onImportFile: handleImportFileBytes,
    onImportError: handleImportError,
  });

  return (
    <div className="flex flex-1 flex-col h-full min-h-0">
      <div className="flex-1 overflow-y-auto min-h-0">
        <div className="max-w-5xl mx-auto w-full px-6 py-8 space-y-5 page-transition">
          {/* Header */}
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h1 className="text-lg font-semibold font-display tracking-tight">
                {t("view.title")}
              </h1>
              <p className="text-xs text-muted-foreground">
                {t("view.description")}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline-flat"
                size="sm"
                onClick={openFilePicker}
              >
                <Upload className="w-3.5 h-3.5" />
                {t("common:actions.import")}
              </Button>
              <Button
                type="button"
                variant="outline-flat"
                size="sm"
                onClick={() => openPersonaEditor()}
              >
                <Plus className="w-3.5 h-3.5" />
                {t("view.newPersona")}
              </Button>
              <input
                ref={fileInputRef}
                type="file"
                accept=".md,text/markdown,text/plain"
                className="hidden"
                onChange={handleFileChange}
              />
            </div>
          </div>

          {/* Search */}
          <SearchBar
            value={search}
            onChange={setSearch}
            placeholder={t("view.searchPlaceholder")}
          />

          {/* Personas section */}
          <section aria-labelledby="personas-heading">
            <PersonaGallery
              personas={filteredPersonas}
              onSelectPersona={(p) => openPersonaEditor(p, "details")}
              onEditPersona={(p) => openPersonaEditor(p, "edit")}
              onDuplicatePersona={handleDuplicatePersona}
              onDeletePersona={handleDeletePersona}
              onExportPersona={handleExportPersona}
              onCreatePersona={() => openPersonaEditor()}
              onImportFile={handleImportFileBytes}
              onImportError={handleImportError}
              isLoading={personasLoading}
            />
          </section>
        </div>
      </div>

      {/* Persona editor modal */}
      <PersonaEditor
        persona={editingPersona ?? undefined}
        isOpen={personaEditorOpen}
        mode={personaEditorMode}
        onClose={closePersonaEditor}
        onSave={handleSavePersona}
        onDuplicate={handleDuplicatePersona}
        onEdit={(persona) => openPersonaEditor(persona, "edit")}
        onDelete={handleDeletePersona}
      />

      {/* Delete confirmation dialog */}
      <AlertDialog
        open={!!deletingPersona}
        onOpenChange={(open) => !open && setDeletingPersona(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("view.deleteTitle", {
                name: deletingPersona?.displayName ?? "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("view.deleteDescription", {
                name: deletingPersona?.displayName ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={handleConfirmDeletePersona}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
