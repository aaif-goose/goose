import { useMemo, useState } from "react";
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
import {
  IconDeviceFloppy,
  IconEye,
  IconEyeOff,
  IconTrash,
} from "@tabler/icons-react";
import type { CustomProviderEngine } from "@/features/providers/lib/customProviderTypes";
import {
  validateCustomProviderDraft,
  type CustomProviderValidationField,
  type CustomProviderValidationIssue,
} from "@/features/providers/lib/customProviderValidation";
import { CustomHeadersEditor, type CustomHeader } from "./CustomHeadersEditor";
import { ProviderModelListEditor } from "./ProviderModelListEditor";

export interface ProviderTemplate {
  id: string;
  displayName: string;
  description?: string;
  engine: string;
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
  engine: string;
  apiUrl: string;
  basePath: string;
  requiresAuth: boolean;
  apiKey: string;
  apiKeySet: boolean;
  models: string[];
  authInitiallyEnabled: boolean;
  supportsStreaming: boolean;
  headers: CustomHeader[];
  catalogProviderId?: string;
}

interface CustomProviderFormProps {
  value: CustomProviderFormValues;
  mode: "create" | "edit";
  saving?: boolean;
  deleting?: boolean;
  error?: string;
  onChange: (value: CustomProviderFormValues) => void;
  onSubmit: () => void;
  onDelete?: () => void;
}

const ENGINE_OPTIONS: CustomProviderEngine[] = [
  "openai_compatible",
  "anthropic_compatible",
  "ollama_compatible",
];

function translationKey(key: string) {
  return key.replace(/^settings\./, "");
}

function fieldIssues(
  issues: CustomProviderValidationIssue[],
  field: CustomProviderValidationField,
) {
  return issues.filter((issue) => issue.field === field);
}

export function CustomProviderForm({
  value,
  mode,
  saving = false,
  deleting = false,
  error = "",
  onChange,
  onSubmit,
  onDelete,
}: CustomProviderFormProps) {
  const { t } = useTranslation(["settings", "common"]);
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const disabled = saving || deleting;
  const validationIssues = useMemo(
    () =>
      validateCustomProviderDraft({
        providerId: value.providerId,
        editable: true,
        engine: value.engine,
        displayName: value.displayName,
        apiUrl: value.apiUrl,
        basePath: value.basePath,
        apiKey: value.apiKey,
        apiKeySet: value.apiKeySet,
        modelsInput: value.models.join(", "),
        models: value.models,
        authInitiallyEnabled: value.authInitiallyEnabled,
        requiresAuth: value.requiresAuth,
        supportsStreaming: value.supportsStreaming,
        headers: value.headers,
        catalogProviderId: value.catalogProviderId,
      }),
    [value],
  );
  const isValid = validationIssues.length === 0;

  function update(patch: Partial<CustomProviderFormValues>) {
    onChange({ ...value, ...patch });
  }

  function renderFieldErrors(field: CustomProviderValidationField) {
    const issues = fieldIssues(validationIssues, field);
    if (issues.length === 0) {
      return null;
    }

    return (
      <div role="alert" className="space-y-0.5 text-xs text-danger">
        {issues.map((issue) => (
          <p key={`${issue.key}-${issue.index ?? "field"}`}>
            {t(translationKey(issue.key))}
          </p>
        ))}
      </div>
    );
  }

  return (
    <form
      className="space-y-5"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
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
            spellCheck={false}
            className="h-8 text-xs"
          />
          {renderFieldErrors("displayName")}
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="custom-provider-engine">
            {t("providers.custom.fields.engine")}
          </Label>
          <Select
            value={value.engine}
            onValueChange={(engine) =>
              update({ engine: engine as CustomProviderEngine })
            }
            disabled={disabled}
          >
            <SelectTrigger
              id="custom-provider-engine"
              className="h-8 w-full text-xs"
            >
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
          {renderFieldErrors("engine")}
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
            spellCheck={false}
            className="h-8 text-xs"
          />
          {renderFieldErrors("apiUrl")}
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
            spellCheck={false}
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
            <div className="flex items-center gap-2">
              <Input
                id="custom-provider-api-key"
                type={apiKeyVisible ? "text" : "password"}
                value={value.apiKey}
                onChange={(event) => update({ apiKey: event.target.value })}
                placeholder={
                  mode === "edit" && value.apiKeySet
                    ? t("providers.custom.fields.apiKeyEditPlaceholder")
                    : t("providers.custom.fields.apiKeyPlaceholder")
                }
                disabled={disabled}
                spellCheck={false}
                autoComplete="new-password"
                data-1p-ignore
                data-lpignore
                className="h-8 text-xs"
              />
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                aria-label={
                  apiKeyVisible
                    ? t("providers.custom.actions.hideApiKey")
                    : t("providers.custom.actions.showApiKey")
                }
                onClick={() => setApiKeyVisible((visible) => !visible)}
                disabled={disabled}
              >
                {apiKeyVisible ? (
                  <IconEyeOff className="size-3.5" />
                ) : (
                  <IconEye className="size-3.5" />
                )}
              </Button>
            </div>
            {renderFieldErrors("apiKey")}
          </div>
        ) : null}
      </section>

      <fieldset className="space-y-2">
        <legend
          id="custom-provider-models-label"
          className="text-sm font-medium"
        >
          {t("providers.custom.fields.models")}
        </legend>
        <ProviderModelListEditor
          models={value.models}
          onChange={(models) => update({ models })}
          disabled={disabled}
        />
        {renderFieldErrors("models")}
      </fieldset>

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

      <fieldset className="space-y-2">
        <legend
          id="custom-provider-headers-label"
          className="text-sm font-medium"
        >
          {t("providers.custom.fields.headers")}
        </legend>
        <CustomHeadersEditor
          headers={value.headers}
          onChange={(headers) => update({ headers })}
          disabled={disabled}
        />
        {renderFieldErrors("headers")}
      </fieldset>

      {error ? (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : null}

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
