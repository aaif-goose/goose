import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Pencil, Trash2 } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import {
  Avatar as AvatarRoot,
  AvatarImage,
  AvatarFallback,
} from "@/shared/ui/avatar";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { Textarea } from "@/shared/ui/textarea";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import type { Persona, ProviderType, Avatar } from "@/shared/types/agents";
import type {
  CreatePersonaRequest,
  UpdatePersonaRequest,
} from "@/shared/types/agents";
import { discoverAcpProviders } from "@/shared/api/acp";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import { getProviderInventory } from "@/features/providers/api/inventory";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import {
  getPersonaSource,
  isPersonaReadOnly,
} from "@/features/agents/lib/personaPresentation";
import { AvatarDropZone } from "./AvatarDropZone";
import { PersonaDetails } from "./PersonaDetails";

interface PersonaEditorProps {
  persona?: Persona;
  isOpen: boolean;
  mode?: "create" | "edit" | "details";
  onClose: () => void;
  onSave: (data: CreatePersonaRequest | UpdatePersonaRequest) => void;
  onDuplicate?: (persona: Persona) => void;
  onEdit?: (persona: Persona) => void;
  onDelete?: (persona: Persona) => void;
  isPending?: boolean;
}

export function PersonaEditor({
  persona,
  isOpen,
  mode = "create",
  onClose,
  onSave,
  onDuplicate,
  onEdit,
  onDelete,
  isPending = false,
}: PersonaEditorProps) {
  const { t } = useTranslation(["agents", "common"]);
  const isEditing = mode === "edit";
  const detailsMode = mode === "details";
  const readOnlyBySource = persona ? isPersonaReadOnly(persona) : false;
  const isReadOnly = detailsMode || readOnlyBySource;
  const personaSource = persona ? getPersonaSource(persona) : "custom";
  const canEditPersona = personaSource === "custom";
  const canDeletePersona = personaSource !== "builtin";
  const acpProviders = useAgentStore((s) => s.providers);
  const setProviders = useAgentStore((s) => s.setProviders);
  const mergeInventoryEntries = useProviderInventoryStore(
    (s) => s.mergeEntries,
  );
  const { getEntry, getModelsForProvider } = useProviderInventory();

  const [displayName, setDisplayName] = useState("");
  const [avatar, setAvatar] = useState<Avatar | null>(null);
  const [systemPrompt, setSystemPrompt] = useState("");
  const [provider, setProvider] = useState<ProviderType | "">("");
  const [model, setModel] = useState("");

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    let cancelled = false;

    const syncProviderOptions = async () => {
      try {
        const providers = await discoverAcpProviders();
        if (!cancelled) {
          setProviders(providers);
        }
      } catch {}

      try {
        const entries = await getProviderInventory();
        if (!cancelled) {
          mergeInventoryEntries(entries);
        }
      } catch {}
    };

    void syncProviderOptions();

    return () => {
      cancelled = true;
    };
  }, [isOpen, mergeInventoryEntries, setProviders]);

  useEffect(() => {
    if (isOpen && persona) {
      setDisplayName(persona.displayName);
      setAvatar(persona.avatar ?? null);
      setSystemPrompt(persona.systemPrompt);
      setProvider(persona.provider ?? "");
      setModel(persona.model ?? "");
    } else if (isOpen) {
      setDisplayName("");
      setAvatar(null);
      setSystemPrompt("");
      setProvider("");
      setModel("");
    }
  }, [isOpen, persona]);

  const isValid =
    displayName.trim().length > 0 && systemPrompt.trim().length > 0;
  const avatarSrc = useAvatarSrc(avatar);

  const availableModels = provider ? getModelsForProvider(provider) : [];
  const providerInventory = provider ? getEntry(provider) : undefined;
  const modelStatusMessage =
    providerInventory?.modelSelectionHint ??
    providerInventory?.lastRefreshError;
  const hasSavedModelOutsideInventory =
    Boolean(model) && !availableModels.some((entry) => entry.id === model);
  const modelSelectValue = hasSavedModelOutsideInventory
    ? `__saved__:${model}`
    : model || "__none__";

  const readOnlyDescription = readOnlyBySource
    ? personaSource === "builtin"
      ? t("editor.readOnlyBuiltIn")
      : t("editor.readOnlyFile")
    : null;
  const providerLabel = provider
    ? (acpProviders.find((providerOption) => providerOption.id === provider)
        ?.label ?? provider)
    : t("common:labels.none");
  const modelLabel = model || t("common:labels.none");

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!isValid || isReadOnly) return;

      const data: CreatePersonaRequest | UpdatePersonaRequest = {
        displayName: displayName.trim(),
        avatar: avatar ?? undefined,
        systemPrompt: systemPrompt.trim(),
        provider: provider || undefined,
        model: model.trim() || undefined,
      };
      onSave(data);
    },
    [
      isValid,
      isReadOnly,
      displayName,
      avatar,
      systemPrompt,
      provider,
      model,
      onSave,
    ],
  );

  const initials = displayName.charAt(0).toUpperCase() || "?";

  // For new personas, use a temporary ID for the avatar upload
  const avatarPersonaId = persona?.id ?? "new-persona";

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-lg max-h-[85vh] flex flex-col gap-0 p-0">
        <DialogHeader className="shrink-0 px-5 py-4">
          <DialogTitle className="text-sm">
            {detailsMode
              ? persona?.displayName
              : isEditing
                ? t("editor.editTitle")
                : t("editor.newTitle")}
          </DialogTitle>
          {readOnlyDescription ? (
            <p className="text-xs text-muted-foreground">
              {readOnlyDescription}
            </p>
          ) : null}
        </DialogHeader>

        {detailsMode ? (
          <PersonaDetails
            avatar={avatar}
            displayName={displayName}
            modelLabel={modelLabel}
            personaSource={personaSource}
            providerLabel={providerLabel}
            systemPrompt={systemPrompt}
          />
        ) : (
          <form
            id="persona-form"
            onSubmit={handleSubmit}
            className="min-h-0 flex-1 overflow-y-auto space-y-4 px-5 pb-5"
          >
            <div className="flex justify-center">
              {isReadOnly ? (
                <AvatarRoot className="h-16 w-16 border border-border">
                  <AvatarImage
                    src={avatarSrc ?? undefined}
                    alt={t("avatar.previewAlt")}
                  />
                  <AvatarFallback className="text-lg font-semibold">
                    {initials}
                  </AvatarFallback>
                </AvatarRoot>
              ) : (
                <AvatarDropZone
                  personaId={avatarPersonaId}
                  avatar={avatar}
                  onChange={setAvatar}
                  disabled={isReadOnly}
                />
              )}
            </div>

            <div className="space-y-1">
              <Label className="text-xs font-medium text-muted-foreground">
                {t("editor.displayName")}{" "}
                <span className="text-destructive">*</span>
              </Label>
              <Input
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                readOnly={isReadOnly}
                required
                placeholder={t("editor.displayNamePlaceholder")}
                className={cn(isReadOnly && "opacity-70 cursor-not-allowed")}
              />
            </div>

            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <Label className="text-xs font-medium text-muted-foreground">
                  {t("editor.systemPrompt")}{" "}
                  <span className="text-destructive">*</span>
                </Label>
                <span className="text-[10px] text-muted-foreground">
                  {t("common:labels.characterCount", {
                    count: systemPrompt.length,
                  })}
                </span>
              </div>
              <Textarea
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                readOnly={isReadOnly}
                required
                rows={6}
                placeholder={t("editor.systemPromptPlaceholder")}
                className={cn(
                  "leading-relaxed",
                  isReadOnly && "opacity-70 cursor-not-allowed",
                )}
              />
            </div>

            <div className="space-y-1">
              <Label className="text-xs font-medium text-muted-foreground">
                {t("editor.provider")}
              </Label>
              <Select
                value={provider || "__none__"}
                onValueChange={(v: string) => {
                  const nextProvider =
                    v === "__none__"
                      ? ("" as ProviderType | "")
                      : (v as ProviderType);
                  setProvider(nextProvider);
                  if (nextProvider !== provider) {
                    setModel("");
                  }
                }}
                disabled={isReadOnly}
              >
                <SelectTrigger
                  className={cn(
                    "w-full",
                    isReadOnly && "opacity-70 cursor-not-allowed",
                  )}
                >
                  <SelectValue placeholder={t("common:labels.none")} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">
                    {t("common:labels.none")}
                  </SelectItem>
                  {acpProviders.map((providerOption) => (
                    <SelectItem
                      key={providerOption.id}
                      value={providerOption.id}
                    >
                      {providerOption.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1">
              <Label className="text-xs font-medium text-muted-foreground">
                {t("editor.model")}
              </Label>
              <Select
                value={modelSelectValue}
                onValueChange={(value: string) => {
                  if (value === "__none__") {
                    setModel("");
                    return;
                  }
                  if (value.startsWith("__saved__:")) {
                    setModel(value.slice("__saved__:".length));
                    return;
                  }
                  setModel(value);
                }}
                disabled={isReadOnly || !provider}
              >
                <SelectTrigger
                  className={cn(
                    "w-full",
                    isReadOnly && "opacity-70 cursor-not-allowed",
                  )}
                >
                  <SelectValue
                    placeholder={
                      provider
                        ? t("editor.modelPlaceholder")
                        : t("editor.chooseProviderFirst")
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">
                    {t("common:labels.none")}
                  </SelectItem>
                  {hasSavedModelOutsideInventory && (
                    <SelectItem value={`__saved__:${model}`}>
                      {t("editor.savedModelUnavailable", { model })}
                    </SelectItem>
                  )}
                  {availableModels.map((modelOption) => (
                    <SelectItem key={modelOption.id} value={modelOption.id}>
                      {modelOption.displayName ?? modelOption.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {hasSavedModelOutsideInventory ? (
                <p className="text-[11px] text-muted-foreground">
                  {t("editor.savedModelUnavailableHelp")}
                </p>
              ) : !provider ? (
                <p className="text-[11px] text-muted-foreground">
                  {t("editor.chooseProviderFirst")}
                </p>
              ) : availableModels.length === 0 ? (
                <p className="text-[11px] text-muted-foreground">
                  {modelStatusMessage ?? t("editor.noModelsAvailable")}
                </p>
              ) : null}
            </div>
          </form>
        )}

        <DialogFooter className="shrink-0 border-t px-5 py-4">
          {detailsMode && persona ? (
            <>
              {onEdit && canEditPersona ? (
                <Button
                  type="button"
                  variant="outline-flat"
                  size="sm"
                  onClick={() => onEdit(persona)}
                >
                  <Pencil className="h-3.5 w-3.5" />
                  {t("common:actions.edit")}
                </Button>
              ) : null}
              {onDuplicate ? (
                <Button
                  type="button"
                  variant="outline-flat"
                  size="sm"
                  onClick={() => onDuplicate(persona)}
                >
                  <Copy className="h-3.5 w-3.5" />
                  {t("editor.duplicate")}
                </Button>
              ) : null}
              {onDelete && canDeletePersona ? (
                <Button
                  type="button"
                  variant="destructive-flat"
                  size="sm"
                  onClick={() => onDelete(persona)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("common:actions.delete")}
                </Button>
              ) : null}
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                {t("common:actions.close")}
              </Button>
            </>
          ) : isReadOnly && onDuplicate && persona ? (
            <>
              <Button
                type="button"
                variant="outline-flat"
                size="sm"
                onClick={() => onDuplicate(persona)}
              >
                <Copy className="h-3.5 w-3.5" />
                {t("editor.duplicate")}
              </Button>
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                {t("common:actions.close")}
              </Button>
            </>
          ) : (
            <>
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                {t("common:actions.cancel")}
              </Button>
              <Button
                type="submit"
                form="persona-form"
                size="sm"
                disabled={!isValid || isPending}
              >
                {isPending
                  ? t("editor.saving")
                  : isEditing
                    ? t("common:actions.saveChanges")
                    : t("editor.create")}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
