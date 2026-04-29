import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Button } from "@/shared/ui/button";
import {
  IconArrowLeft,
  IconLayoutGrid,
  IconSettings,
} from "@tabler/icons-react";
import {
  CustomProviderForm,
  type CustomProviderFormValues,
  type ProviderTemplate,
} from "./CustomProviderForm";
import { ProviderTemplatePicker } from "./ProviderTemplatePicker";

export type CustomProviderMutationInput = Omit<
  CustomProviderFormValues,
  "providerId"
> & {
  providerId?: string;
};

interface CustomProviderDialogProps {
  open: boolean;
  mode: "create" | "edit";
  provider?: CustomProviderFormValues | null;
  templates?: ProviderTemplate[];
  onOpenChange: (open: boolean) => void;
  onCreate: (input: CustomProviderMutationInput) => Promise<void>;
  onUpdate: (
    providerId: string,
    input: CustomProviderMutationInput,
  ) => Promise<void>;
  onDelete?: (providerId: string) => Promise<boolean | undefined>;
}

const EMPTY_FORM: CustomProviderFormValues = {
  displayName: "",
  engine: "openai_compatible",
  apiUrl: "",
  basePath: "",
  requiresAuth: true,
  apiKey: "",
  apiKeySet: false,
  models: [],
  authInitiallyEnabled: true,
  supportsStreaming: true,
  headers: [],
};

type CreateStep = "choice" | "template" | "form";

function valueFromTemplate(
  template: ProviderTemplate,
): CustomProviderFormValues {
  return {
    ...EMPTY_FORM,
    displayName: template.displayName,
    engine: template.engine,
    apiUrl: template.apiUrl,
    basePath: template.basePath ?? "",
    requiresAuth: template.requiresAuth,
    authInitiallyEnabled: template.requiresAuth,
    models: template.models,
    supportsStreaming: template.supportsStreaming,
    headers: template.headers,
    catalogProviderId: template.id,
  };
}

export function CustomProviderDialog({
  open,
  mode,
  provider,
  templates = [],
  onOpenChange,
  onCreate,
  onUpdate,
  onDelete,
}: CustomProviderDialogProps) {
  const { t } = useTranslation("settings");
  const [value, setValue] = useState<CustomProviderFormValues>(EMPTY_FORM);
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(
    null,
  );
  const [createStep, setCreateStep] = useState<CreateStep>("choice");
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState("");
  const openStateKeyRef = useRef<string | null>(null);
  const templateById = useMemo(
    () => new Map(templates.map((template) => [template.id, template])),
    [templates],
  );

  useEffect(() => {
    if (!open) {
      openStateKeyRef.current = null;
      return;
    }
    const openStateKey = `${mode}:${provider?.providerId ?? "new"}`;
    if (openStateKeyRef.current === openStateKey) {
      return;
    }
    openStateKeyRef.current = openStateKey;
    setValue(provider ?? EMPTY_FORM);
    setSelectedTemplateId(provider?.catalogProviderId ?? null);
    setCreateStep(mode === "create" ? "choice" : "form");
    setSaving(false);
    setDeleting(false);
    setError("");
  }, [mode, open, provider]);

  function handleStartManual() {
    setSelectedTemplateId(null);
    setValue(EMPTY_FORM);
    setCreateStep("form");
  }

  function handleSelectTemplate(templateId: string) {
    setSelectedTemplateId(templateId);
    const template = templateById.get(templateId);
    setValue(template ? valueFromTemplate(template) : EMPTY_FORM);
    setCreateStep("form");
  }

  function handleBack() {
    setError("");
    if (createStep === "template") {
      setCreateStep("choice");
      return;
    }
    if (selectedTemplateId) {
      setCreateStep("template");
      return;
    }
    setCreateStep("choice");
  }

  async function handleSubmit() {
    setSaving(true);
    setError("");
    try {
      if (mode === "edit" && value.providerId) {
        await onUpdate(value.providerId, value);
      } else {
        await onCreate(value);
      }
      onOpenChange(false);
    } catch (nextError) {
      setError(
        nextError instanceof Error
          ? nextError.message
          : t("providers.custom.errors.saveFailed"),
      );
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!value.providerId || !onDelete) {
      return;
    }

    setDeleting(true);
    setError("");
    try {
      const deleted = await onDelete(value.providerId);
      if (deleted === false) {
        return;
      }
      onOpenChange(false);
    } catch (nextError) {
      setError(
        nextError instanceof Error
          ? nextError.message
          : t("providers.custom.errors.deleteFailed"),
      );
    } finally {
      setDeleting(false);
    }
  }

  function renderCreateChoice() {
    return (
      <div className="grid gap-3 sm:grid-cols-2">
        <button
          type="button"
          onClick={handleStartManual}
          className="flex min-h-24 items-start gap-3 rounded-lg border border-border px-3 py-3 text-left transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <IconSettings className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0">
            <span className="block text-sm font-medium">
              {t("providers.custom.templates.manual")}
            </span>
            <span className="mt-1 block text-xs text-muted-foreground">
              {t("providers.custom.templates.manualDescription")}
            </span>
          </span>
        </button>

        <button
          type="button"
          onClick={() => setCreateStep("template")}
          className="flex min-h-24 items-start gap-3 rounded-lg border border-border px-3 py-3 text-left transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <IconLayoutGrid className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0">
            <span className="block text-sm font-medium">
              {t("providers.custom.templates.useTemplate")}
            </span>
            <span className="mt-1 block text-xs text-muted-foreground">
              {t("providers.custom.templates.useTemplateDescription")}
            </span>
          </span>
        </button>
      </div>
    );
  }

  function renderBackButton() {
    if (mode !== "create" || createStep === "choice") {
      return null;
    }

    return (
      <div className="-mt-1 mb-2 flex justify-start">
        <Button
          type="button"
          variant="inline-subtle"
          size="xs"
          onClick={handleBack}
          leftIcon={<IconArrowLeft />}
          className="px-1.5"
        >
          {t("providers.custom.actions.back")}
        </Button>
      </div>
    );
  }

  function renderContent() {
    if (mode === "create" && createStep === "choice") {
      return renderCreateChoice();
    }

    if (mode === "create" && createStep === "template") {
      return (
        <>
          {renderBackButton()}
          <ProviderTemplatePicker
            templates={templates}
            onSelect={handleSelectTemplate}
            disabled={saving || deleting}
          />
        </>
      );
    }

    return (
      <>
        {renderBackButton()}
        <CustomProviderForm
          value={value}
          mode={mode}
          saving={saving}
          deleting={deleting}
          error={error}
          onChange={setValue}
          onSubmit={() => void handleSubmit()}
          onDelete={
            mode === "edit" && onDelete ? () => void handleDelete() : undefined
          }
        />
      </>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[min(760px,calc(100vh-2rem))] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {mode === "edit"
              ? t("providers.custom.editTitle")
              : t("providers.custom.addTitle")}
          </DialogTitle>
          <DialogDescription>
            {t("providers.custom.description")}
          </DialogDescription>
        </DialogHeader>

        {renderContent()}
      </DialogContent>
    </Dialog>
  );
}
