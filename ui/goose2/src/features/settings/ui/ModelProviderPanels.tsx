import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { AsyncButton } from "@/shared/ui/async-button";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { IconLoader2 } from "@tabler/icons-react";
import type {
  ProviderField,
  ProviderFieldValue,
  ProviderSetupMethod,
} from "@/shared/types/providers";
import {
  resolveFieldValue,
  getDisplayValue,
  renderSetupMessage,
  renderInlineCodeMessage,
} from "./modelProviderHelpers";

interface ConnectedFieldsPanelProps {
  panelRef: RefObject<HTMLDivElement | null>;
  fields: ProviderField[];
  fieldValueMap: Map<string, ProviderFieldValue>;
  editingKey: string | null;
  draftValues: Record<string, string>;
  saving: boolean;
  showSavedState: boolean;
  error: string;
  setupMessage: string | null;
  onStartEdit: (key: string) => void;
  onCancelEdit: (field: ProviderField) => void;
  onDraftChange: (key: string, value: string) => void;
  onSaveField: (field: ProviderField) => void;
  onRemove: () => void;
}

export function ConnectedFieldsPanel({
  panelRef,
  fields,
  fieldValueMap,
  editingKey,
  draftValues,
  saving,
  showSavedState,
  error,
  setupMessage,
  onStartEdit,
  onCancelEdit,
  onDraftChange,
  onSaveField,
  onRemove,
}: ConnectedFieldsPanelProps) {
  const { t } = useTranslation(["settings", "common"]);
  return (
    <div
      ref={panelRef}
      tabIndex={-1}
      className="focus-override mx-3 space-y-3 rounded-b-lg border-x border-b px-3 py-3 outline-none"
    >
      {fields.map((field) => {
        const isEditing = editingKey === field.key;
        return (
          <div
            key={field.key}
            className="space-y-2 rounded-md border border-border px-3 py-2.5"
          >
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="text-sm">{field.label}</p>
                {!isEditing && (
                  <p className="truncate text-xs text-muted-foreground">
                    {getDisplayValue(field, fieldValueMap, t)}
                  </p>
                )}
              </div>

              {!isEditing && (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() => onStartEdit(field.key)}
                  disabled={saving}
                  className="text-muted-foreground"
                >
                  {resolveFieldValue(field, fieldValueMap).isSet
                    ? "Edit"
                    : "Add"}
                </Button>
              )}
            </div>

            {isEditing && (
              <div className="flex items-center gap-2">
                <Input
                  type={field.secret ? "password" : "text"}
                  value={draftValues[field.key] ?? ""}
                  placeholder={
                    field.secret &&
                    resolveFieldValue(field, fieldValueMap).isSet
                      ? getDisplayValue(field, fieldValueMap, t)
                      : field.placeholder
                  }
                  onChange={(event) =>
                    onDraftChange(field.key, event.target.value)
                  }
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      onSaveField(field);
                    }
                  }}
                  disabled={saving}
                  className="h-8 flex-1 text-xs"
                />
                <Button
                  type="button"
                  size="sm"
                  onClick={() => onSaveField(field)}
                  disabled={saving || !(draftValues[field.key]?.trim() ?? "")}
                  className="h-8"
                >
                  {saving ? (
                    <IconLoader2 className="size-3 animate-spin" />
                  ) : null}
                  {t("common:actions.save")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => onCancelEdit(field)}
                  disabled={saving}
                  className="h-8"
                >
                  {t("common:actions.cancel")}
                </Button>
              </div>
            )}
          </div>
        );
      })}

      <div className="flex justify-end gap-2">
        {showSavedState ? (
          <Button type="button" variant="secondary" size="sm" disabled>
            {t("providers.saved")}
          </Button>
        ) : null}
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => onRemove()}
          disabled={saving}
          className="text-danger hover:text-danger"
        >
          {saving ? <IconLoader2 className="size-3 animate-spin" /> : null}
          {t("providers.disconnect")}
        </Button>
      </div>
      {renderSetupMessage(setupMessage)}
      {error && <p className="text-xs text-danger">{error}</p>}
    </div>
  );
}

interface SetupFieldsPanelProps {
  panelRef: RefObject<HTMLDivElement | null>;
  fields: ProviderField[];
  fieldValueMap: Map<string, ProviderFieldValue>;
  draftValues: Record<string, string>;
  saving: boolean;
  showSavedState: boolean;
  error: string;
  setupMethod: ProviderSetupMethod;
  setupMessage: string | null;
  fieldSetupDescription: string | null;
  isConnected: boolean;
  onDraftChange: (key: string, value: string) => void;
  onSaveSetup: () => void;
}

export function SetupFieldsPanel({
  panelRef,
  fields,
  fieldValueMap,
  draftValues,
  saving,
  showSavedState,
  error,
  setupMethod,
  setupMessage,
  fieldSetupDescription,
  isConnected,
  onDraftChange,
  onSaveSetup,
}: SetupFieldsPanelProps) {
  const { t } = useTranslation(["settings", "common"]);
  return (
    <div
      ref={panelRef}
      tabIndex={-1}
      className="focus-override mx-3 space-y-3 rounded-b-lg border-x border-b px-3 py-3 outline-none"
    >
      {!isConnected && fieldSetupDescription ? (
        <p className="text-xs text-muted-foreground">{fieldSetupDescription}</p>
      ) : null}
      {fields.map((field) => {
        const fieldValue = resolveFieldValue(field, fieldValueMap);
        return (
          <div key={field.key} className="space-y-1">
            <div className="flex items-center justify-between gap-3">
              <span className="text-xs font-medium text-foreground">
                {field.label}
              </span>
              {field.required && (
                <span className="text-xxs text-muted-foreground">
                  {t("common:labels.required")}
                </span>
              )}
            </div>
            <Input
              type={field.secret ? "password" : "text"}
              value={draftValues[field.key] ?? ""}
              placeholder={
                field.secret && fieldValue.isSet
                  ? getDisplayValue(field, fieldValueMap, t)
                  : field.placeholder
              }
              onChange={(event) => onDraftChange(field.key, event.target.value)}
              disabled={saving}
              className="h-8 text-xs"
            />
          </div>
        );
      })}

      <div className="flex justify-end">
        <AsyncButton
          type="button"
          state={saving ? "pending" : showSavedState ? "success" : "idle"}
          idleLabel="Save"
          pendingLabel="Saving..."
          successLabel="Saved"
          pendingVisual="text"
          pendingDelayMs={250}
          size="sm"
          onClick={() => onSaveSetup()}
          disabled={saving || showSavedState}
          className="h-8"
        />
      </div>
      {setupMethod === "host_with_oauth_fallback"
        ? renderInlineCodeMessage(
            t("providers.models.setup.hostWithOauthFallbackTerminal"),
          )
        : null}
      {setupMethod === "cloud_credentials" && setupMessage
        ? renderSetupMessage(setupMessage)
        : null}
      {error && <p className="text-xs text-danger">{error}</p>}
    </div>
  );
}
