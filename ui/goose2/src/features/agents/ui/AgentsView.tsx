import { useState, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { Plus, Upload } from "lucide-react";
import { toast } from "sonner";
import { SearchBar } from "@/shared/ui/SearchBar";
import { Button, buttonVariants } from "@/shared/ui/button";
import { PageHeader, PageShell } from "@/shared/ui/page-shell";
import { copyFileToClipboard, saveFileCopy } from "@/shared/api/system";
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
import { AgentDetailPage } from "@/features/agents/ui/AgentDetailPage";
import { PersonaGallery } from "@/features/agents/ui/PersonaGallery";
import { PersonaEditor } from "@/features/agents/ui/PersonaEditor";
import { importPersonas, readImportPersonaFile } from "@/shared/api/agents";
import { usePersonas } from "@/features/agents/hooks/usePersonas";
import type {
  Persona,
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";
import {
  formatAgentError,
  formatImportSuccessMessage,
  validatePersonaImportFile,
} from "@/features/agents/lib/personaImport";

interface AgentsViewProps {
  onStartChatWithPersona?: (persona: Persona) => void;
}

export function AgentsView({ onStartChatWithPersona }: AgentsViewProps) {
  const { t } = useTranslation(["agents", "common"]);
  const [search, setSearch] = useState("");
  const [deletingPersona, setDeletingPersona] = useState<Persona | null>(null);
  const [activePersonaId, setActivePersonaId] = useState<string | null>(null);

  const personas = useAgentStore((s) => s.personas);
  const personasLoading = useAgentStore((s) => s.personasLoading);
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
  const activePersona =
    personas.find((persona) => persona.id === activePersonaId) ?? null;

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
    setDeletingPersona(persona);
  }, []);

  const handleConfirmDeletePersona = useCallback(async () => {
    if (!deletingPersona) return;
    try {
      await deletePersona(deletingPersona.id);
      if (editingPersona?.id === deletingPersona.id) {
        closePersonaEditor();
      }
      if (activePersonaId === deletingPersona.id) {
        setActivePersonaId(null);
      }
      toast.success(t("view.deleted", { name: deletingPersona.displayName }));
    } catch (err) {
      toast.error(formatAgentError(err, t("view.deleteFailed")));
    }
    setDeletingPersona(null);
  }, [
    activePersonaId,
    closePersonaEditor,
    deletingPersona,
    deletePersona,
    editingPersona,
    t,
  ]);

  const handleCopyPersonaFile = useCallback(
    async (persona: Persona) => {
      if (!persona.sourcePath) return;
      try {
        await copyFileToClipboard(persona.sourcePath);
        toast.success(t("view.fileCopied"));
      } catch (err) {
        toast.error(formatAgentError(err, t("view.copyFileFailed")));
      }
    },
    [t],
  );

  const handleSavePersonaCopy = useCallback(
    async (persona: Persona) => {
      if (!persona.sourcePath) return;
      try {
        const savedPath = await saveFileCopy(persona.sourcePath);
        if (savedPath) {
          toast.success(t("view.copySaved", { path: savedPath }));
        }
      } catch (err) {
        toast.error(formatAgentError(err, t("view.saveCopyFailed")));
      }
    },
    [t],
  );

  const handleImportError = useCallback((message: string) => {
    toast.error(message);
  }, []);

  const validateImportFile = useCallback(
    (file: Pick<File, "name" | "type">) => {
      const message = validatePersonaImportFile(file);
      return message ? t(message.key, message.options) : null;
    },
    [t],
  );

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

  const handleImportPicker = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t("common:actions.import"),
        filters: [
          {
            name: "Markdown",
            extensions: ["md"],
          },
        ],
      });

      if (!selected || Array.isArray(selected)) {
        return;
      }

      const { fileBytes, fileName } = await readImportPersonaFile(selected);
      const validationMessage = validateImportFile({
        name: fileName,
        type: "",
      });

      if (validationMessage) {
        toast.error(validationMessage);
        return;
      }

      await handleImportFileBytes(fileBytes, fileName);
    } catch (err) {
      toast.error(formatAgentError(err, t("view.importFailed")));
    }
  }, [handleImportFileBytes, t, validateImportFile]);

  const dialogs = (
    <>
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
    </>
  );

  if (activePersona) {
    return (
      <>
        <AgentDetailPage
          persona={activePersona}
          onBack={() => setActivePersonaId(null)}
          onEdit={(persona) => openPersonaEditor(persona, "edit")}
          onStartChat={onStartChatWithPersona}
          onCopyFile={handleCopyPersonaFile}
          onSaveCopy={handleSavePersonaCopy}
          onDuplicate={handleDuplicatePersona}
          onDelete={handleDeletePersona}
        />
        {dialogs}
      </>
    );
  }

  return (
    <PageShell>
      <PageHeader
        title={t("view.title")}
        description={t("view.description")}
        titleClassName="font-normal text-foreground"
        actions={
          <>
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={() => void handleImportPicker()}
            >
              <Upload className="size-3.5" />
              {t("common:actions.import")}
            </Button>
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={() => openPersonaEditor()}
            >
              <Plus className="size-3.5" />
              {t("view.newPersona")}
            </Button>
          </>
        }
      />

      <SearchBar
        value={search}
        onChange={setSearch}
        placeholder={t("view.searchPlaceholder")}
        aria-label={t("view.searchPlaceholder")}
      />

      <section aria-labelledby="personas-heading">
        <PersonaGallery
          personas={filteredPersonas}
          hasAnyPersonas={personas.length > 0}
          onSelectPersona={(p) => setActivePersonaId(p.id)}
          onStartChatPersona={onStartChatWithPersona}
          onEditPersona={(p) => openPersonaEditor(p, "edit")}
          onDuplicatePersona={handleDuplicatePersona}
          onDeletePersona={handleDeletePersona}
          onCopyPersonaFile={handleCopyPersonaFile}
          onSavePersonaCopy={handleSavePersonaCopy}
          onCreatePersona={() => openPersonaEditor()}
          onImportFile={handleImportFileBytes}
          validateImportFile={validateImportFile}
          onImportError={handleImportError}
          isLoading={personasLoading}
        />
      </section>

      {dialogs}
    </PageShell>
  );
}
