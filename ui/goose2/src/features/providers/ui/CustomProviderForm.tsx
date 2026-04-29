import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import { Switch } from "@/shared/ui/switch";
import { IconDeviceFloppy, IconTrash } from "@tabler/icons-react";
import type { CustomProviderEngine } from "@/features/providers/lib/customProviderTypes";
import { CustomHeadersEditor, type CustomHeader } from "./CustomHeadersEditor";
import { ProviderModelListEditor } from "./ProviderModelListEditor";
import { ProviderTemplatePicker } from "./ProviderTemplatePicker";

export interface ProviderTemplate {
  id: string;
  displayName: string;
  description?: string;
  engine: CustomProviderEngine;
  apiUrl: string;
  basePath?: string;
  requiresAuth: boolean;
  supportsStreaming: boolean;
  models: string[];
  headers: CustomHeader[];
}

export interface CustomProviderFormValues {
  providerId?: string;
  displayName: string;
  engine: CustomProviderEngine;
  apiUrl: string;
  basePath: string;
  requiresAuth: boolean;
  apiKey: string;
  models: string[];
  supportsStreaming: boolean;
  headers: CustomHeader[];
  catalogProviderId?: string;
}

interface CustomProviderFormProps {
  value: CustomProviderFormValues;
  mode: "create" | "edit";
  templates: ProviderTemplate[];
  selectedTemplateId: string | null;
  saving?: boolean;
  deleting?: boolean;
  error?: string;
  onChange: (value: CustomProviderFormValues) => void;
  onSelectTemplate: (templateId: string | null) => void;
  onSubmit: () => void;
  onDelete?: () => void;
}

const ENGINE_OPTIONS: CustomProviderEngine[] = [
  "openai_compatible",
  "anthropic_compatible",
  "ollama_compatible",
];

function cleanHeaders(headers: CustomHeader[]) {
  return headers.filter((header) => header.key.trim() && header.value.trim());
}

export function customProviderFormIsValid(value: CustomProviderFormValues) {
  return (
    value.displayName.trim().length > 0 &&
    value.apiUrl.trim().length > 0 &&
    value.models.length > 0 &&
    cleanHeaders(value.headers).length === value.headers.length
  );
}

export function CustomProviderForm({
  value,
  mode,
  templates,
  selectedTemplateId,
  saving = false,
  deleting = false,
  error = "",
  onChange,
  onSelectTemplate,
  onSubmit,
  onDelete,
}: CustomProviderFormProps) {
  const { t } = useTranslation(["settings", "common"]);
  const disabled = saving || deleting;
  const isValid = useMemo(() => customProviderFormIsValid(value), [value]);

  function update(patch: Partial<CustomProviderFormValues>) {
    onChange({ ...value, ...patch });
  }

  return (
    <form
      className="space-y-5"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      {mode === "create" ? (
        <section className="space-y-2">
          <Label>{t("providers.custom.sections.template")}</Label>
          <ProviderTemplatePicker
            templates={templates}
            selectedTemplateId={selectedTemplateId}
            onSelect={onSelectTemplate}
            disabled={disabled}
          />
        </section>
      ) : null}

      <section className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="custom-provider-name">
            {t("providers.custom.fields.displayName")}
          </Label>
          <Input
            id="custom-provider-name"
            value={value.displayName}
            onChange={(event) => update({ displayName: event.target.value })}
            placeholder={t("providers.custom.fields.displayNamePlaceholder")}
            disabled={disabled}
            className="h-8 text-xs"
          />
        </div>

        <div className="space-y-1.5">
          <Label>{t("providers.custom.fields.engine")}</Label>
          <Select
            value={value.engine}
            onValueChange={(engine) =>
              update({ engine: engine as CustomProviderEngine })
            }
            disabled={disabled}
          >
            <SelectTrigger className="h-8 w-full text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ENGINE_OPTIONS.map((engine) => (
                <SelectItem key={engine} value={engine}>
                  {t(`providers.custom.engines.${engine}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="custom-provider-api-url">
            {t("providers.custom.fields.apiUrl")}
          </Label>
          <Input
            id="custom-provider-api-url"
            value={value.apiUrl}
            onChange={(event) => update({ apiUrl: event.target.value })}
            placeholder={t("providers.custom.fields.apiUrlPlaceholder")}
            disabled={disabled}
            className="h-8 text-xs"
          />
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="custom-provider-base-path">
            {t("providers.custom.fields.basePath")}
          </Label>
          <Input
            id="custom-provider-base-path"
            value={value.basePath}
            onChange={(event) => update({ basePath: event.target.value })}
            placeholder={t("providers.custom.fields.basePathPlaceholder")}
            disabled={disabled}
            className="h-8 text-xs"
          />
        </div>
      </section>

      <section className="space-y-3">
        <div className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2">
          <div>
            <Label htmlFor="custom-provider-auth">
              {t("providers.custom.fields.requiresAuth")}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t("providers.custom.fields.requiresAuthDescription")}
            </p>
          </div>
          <Switch
            id="custom-provider-auth"
            checked={value.requiresAuth}
            onCheckedChange={(requiresAuth) => update({ requiresAuth })}
            disabled={disabled}
          />
        </div>

        {value.requiresAuth ? (
          <div className="space-y-1.5">
            <Label htmlFor="custom-provider-api-key">
              {t("providers.custom.fields.apiKey")}
            </Label>
            <Input
              id="custom-provider-api-key"
              type="password"
              value={value.apiKey}
              onChange={(event) => update({ apiKey: event.target.value })}
              placeholder={
                mode === "edit"
                  ? t("providers.custom.fields.apiKeyEditPlaceholder")
                  : t("providers.custom.fields.apiKeyPlaceholder")
              }
              disabled={disabled}
              className="h-8 text-xs"
            />
          </div>
        ) : null}
      </section>

      <section className="space-y-2">
        <Label>{t("providers.custom.fields.models")}</Label>
        <ProviderModelListEditor
          models={value.models}
          onChange={(models) => update({ models })}
          disabled={disabled}
        />
      </section>

      <section className="space-y-3">
        <div className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2">
          <div>
            <Label htmlFor="custom-provider-streaming">
              {t("providers.custom.fields.supportsStreaming")}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t("providers.custom.fields.supportsStreamingDescription")}
            </p>
          </div>
          <Switch
            id="custom-provider-streaming"
            checked={value.supportsStreaming}
            onCheckedChange={(supportsStreaming) =>
              update({ supportsStreaming })
            }
            disabled={disabled}
          />
        </div>
      </section>

      <section className="space-y-2">
        <Label>{t("providers.custom.fields.headers")}</Label>
        <CustomHeadersEditor
          headers={value.headers}
          onChange={(headers) => update({ headers })}
          disabled={disabled}
        />
      </section>

      {error ? <p className="text-xs text-danger">{error}</p> : null}

      <div className="flex items-center justify-between gap-3">
        {mode === "edit" && onDelete ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onDelete}
            disabled={disabled}
            leftIcon={<IconTrash />}
            className="text-danger hover:text-danger"
          >
            {deleting
              ? t("providers.custom.actions.deleting")
              : t("providers.custom.actions.delete")}
          </Button>
        ) : (
          <span />
        )}

        <Button
          type="submit"
          size="sm"
          disabled={disabled || !isValid}
          leftIcon={<IconDeviceFloppy />}
        >
          {saving
            ? t("providers.custom.actions.saving")
            : mode === "edit"
              ? t("providers.custom.actions.save")
              : t("providers.custom.actions.create")}
        </Button>
      </div>
    </form>
  );
}
