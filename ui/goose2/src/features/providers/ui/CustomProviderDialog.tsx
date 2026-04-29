import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  CustomProviderForm,
  type CustomProviderFormValues,
  type ProviderTemplate,
} from "./CustomProviderForm";

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
  onDelete?: (providerId: string) => Promise<void>;
}

const EMPTY_FORM: CustomProviderFormValues = {
  displayName: "",
  engine: "openai_compatible",
  apiUrl: "",
  basePath: "",
  requiresAuth: true,
  apiKey: "",
  models: [],
  supportsStreaming: true,
  headers: [],
};

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
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState("");
  const templateById = useMemo(
    () => new Map(templates.map((template) => [template.id, template])),
    [templates],
  );

  useEffect(() => {
    if (!open) return;
    setValue(provider ?? EMPTY_FORM);
    setSelectedTemplateId(provider?.catalogProviderId ?? null);
    setSaving(false);
    setDeleting(false);
    setError("");
  }, [open, provider]);

  function handleSelectTemplate(templateId: string | null) {
    setSelectedTemplateId(templateId);
    const template = templateId ? templateById.get(templateId) : null;
    setValue(template ? valueFromTemplate(template) : EMPTY_FORM);
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
      await onDelete(value.providerId);
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

        <CustomProviderForm
          value={value}
          mode={mode}
          templates={templates}
          selectedTemplateId={selectedTemplateId}
          saving={saving}
          deleting={deleting}
          error={error}
          onChange={setValue}
          onSelectTemplate={handleSelectTemplate}
          onSubmit={() => void handleSubmit()}
          onDelete={
            mode === "edit" && onDelete ? () => void handleDelete() : undefined
          }
        />
      </DialogContent>
    </Dialog>
  );
}
