import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";
import { Spinner } from "@/shared/ui/spinner";
import {
  getProviderIcon,
  formatProviderLabel,
} from "@/shared/ui/icons/ProviderIcons";
import { IconCheck } from "@tabler/icons-react";
import {
  authenticateModelProvider,
  onModelSetupOutput,
} from "@/features/providers/api/modelSetup";
import type {
  ProviderDisplayInfo,
  ProviderField,
  ProviderFieldValue,
} from "@/shared/types/providers";
import {
  MAX_SETUP_OUTPUT_LINES,
  type SetupOutputLine,
  resolveFieldValue,
  createDraftValues,
  getSetupMessage,
  getNativeConnectDescription,
  getFieldSetupDescription,
  renderSetupMessage,
} from "./modelProviderHelpers";
import { ConnectedFieldsPanel, SetupFieldsPanel } from "./ModelProviderPanels";

interface ModelProviderRowProps {
  provider: ProviderDisplayInfo;
  onGetConfig: (providerId: string) => Promise<ProviderFieldValue[]>;
  onSaveField: (key: string, value: string, isSecret: boolean) => Promise<void>;
  onRemoveConfig?: () => Promise<void>;
  onCompleteNativeSetup: () => Promise<void>;
  saving?: boolean;
}

export function ModelProviderRow({
  provider,
  onGetConfig,
  onSaveField,
  onRemoveConfig,
  onCompleteNativeSetup,
  saving = false,
}: ModelProviderRowProps) {
  const { t } = useTranslation("settings");
  const [expanded, setExpanded] = useState(false);
  const [configValues, setConfigValues] = useState<ProviderFieldValue[]>([]);
  const [draftValues, setDraftValues] = useState<Record<string, string>>({});
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [loadingConfig, setLoadingConfig] = useState(false);
  const [error, setError] = useState("");
  const [authenticating, setAuthenticating] = useState(false);
  const [setupOutput, setSetupOutput] = useState<SetupOutputLine[]>([]);
  const [setupError, setSetupError] = useState("");
  const [showSavedState, setShowSavedState] = useState(false);
  const [preserveSetupLayout, setPreserveSetupLayout] = useState(false);
  const setupLineCounter = useRef(0);
  const hasLoadedConfig = useRef(false);
  const shouldRestorePanelFocus = useRef(false);
  const panelRef = useRef<HTMLDivElement | null>(null);

  const icon = getProviderIcon(provider.id, "size-4");
  const fields = provider.fields ?? [];
  const hasFields = fields.length > 0;
  const supportsNativeConnect = !!provider.nativeConnectQuery;
  const isConnected =
    provider.status === "connected" || provider.status === "built_in";
  const fieldValueMap = useMemo(
    () => new Map(configValues.map((value) => [value.key, value])),
    [configValues],
  );

  const loadConfig = useCallback(
    async ({ showSkeleton = false }: { showSkeleton?: boolean } = {}) => {
      if (!hasFields) return;
      if (showSkeleton) {
        setLoadingConfig(true);
      }
      try {
        const nextValues = await onGetConfig(provider.id);
        hasLoadedConfig.current = true;
        setConfigValues(nextValues);
        setDraftValues(createDraftValues(fields, nextValues));
        setError("");
      } catch (nextError) {
        setError(
          nextError instanceof Error
            ? nextError.message
            : "Failed to load provider settings",
        );
      } finally {
        if (showSkeleton) {
          setLoadingConfig(false);
        }
      }
    },
    [fields, hasFields, onGetConfig, provider.id],
  );

  useEffect(() => {
    if (expanded && hasFields) {
      void loadConfig({ showSkeleton: !hasLoadedConfig.current });
    }
  }, [expanded, hasFields, loadConfig]);

  useEffect(() => {
    if (isConnected) {
      setAuthenticating(false);
      setSetupError("");
    }
  }, [isConnected]);

  useLayoutEffect(() => {
    if (!shouldRestorePanelFocus.current) {
      return;
    }

    shouldRestorePanelFocus.current = false;
    panelRef.current?.focus({ preventScroll: true });
  });

  function appendSetupOutput(line: string) {
    setupLineCounter.current += 1;
    setSetupOutput((current) =>
      [
        ...current,
        {
          id: setupLineCounter.current,
          text: line,
        },
      ].slice(-MAX_SETUP_OUTPUT_LINES),
    );
  }

  async function runNativeConnect() {
    if (!provider.nativeConnectQuery) {
      return;
    }

    setExpanded(true);
    setAuthenticating(true);
    setSetupError("");
    setSetupOutput([]);
    setupLineCounter.current = 0;
    setEditingKey(null);
    setError("");
    setShowSavedState(false);
    setPreserveSetupLayout(false);

    const unlisten = await onModelSetupOutput(provider.id, appendSetupOutput);

    try {
      await authenticateModelProvider(provider.id, provider.nativeConnectQuery);
      await onCompleteNativeSetup();
    } catch (nextError) {
      setSetupError(
        nextError instanceof Error
          ? nextError.message
          : "Failed to complete sign-in",
      );
    } finally {
      unlisten();
      setAuthenticating(false);
    }
  }

  function handleToggle() {
    setExpanded((current) => {
      if (current) {
        setShowSavedState(false);
        setPreserveSetupLayout(false);
      }
      return !current;
    });
    setEditingKey(null);
    setError("");
    setSetupError("");
  }

  function handleStartEdit(key: string) {
    setEditingKey(key);
    setError("");
    setShowSavedState(false);
  }

  function handleCancelEdit(field: ProviderField) {
    setDraftValues((current) => ({
      ...current,
      [field.key]: field.secret
        ? ""
        : (resolveFieldValue(field, fieldValueMap).value ?? ""),
    }));
    setEditingKey(null);
    setError("");
  }

  function handleDraftChange(key: string, value: string) {
    setShowSavedState(false);
    setDraftValues((current) => ({ ...current, [key]: value }));
  }

  async function handleSaveField(field: ProviderField) {
    const nextValue = draftValues[field.key]?.trim() ?? "";
    if (!nextValue) {
      setError(`Enter a value for ${field.label}`);
      return;
    }
    setError("");
    try {
      shouldRestorePanelFocus.current = true;
      await onSaveField(field.key, nextValue, field.secret);
      await loadConfig();
      setEditingKey(null);
      setShowSavedState(true);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : "Failed to save",
      );
    }
  }

  async function handleSaveSetup() {
    const missingLabels = fields
      .filter((field) => {
        if (!field.required) {
          return false;
        }
        const currentValue = resolveFieldValue(field, fieldValueMap);
        const nextValue = draftValues[field.key]?.trim() ?? "";
        return !currentValue.isSet && !nextValue;
      })
      .map((field) => field.label);

    if (missingLabels.length > 0) {
      setError(`Fill in ${missingLabels.join(", ")}`);
      return;
    }

    const fieldsToSave = fields.filter((field) => {
      const currentValue = resolveFieldValue(field, fieldValueMap);
      const nextValue = draftValues[field.key]?.trim() ?? "";

      if (!nextValue) {
        return false;
      }

      if (field.secret) {
        return true;
      }

      return nextValue !== (currentValue.value ?? "");
    });

    if (fieldsToSave.length === 0) {
      setError("");
      return;
    }

    setError("");
    try {
      for (const field of fieldsToSave) {
        const nextValue = draftValues[field.key]?.trim() ?? "";
        await onSaveField(field.key, nextValue, field.secret);
      }
      await loadConfig();
      setShowSavedState(true);
      setPreserveSetupLayout(true);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : "Failed to save",
      );
    }
  }

  async function handleRemove() {
    try {
      shouldRestorePanelFocus.current = true;
      await onRemoveConfig?.();
      await loadConfig();
      setEditingKey(null);
      setError("");
      setShowSavedState(false);
      setPreserveSetupLayout(false);
    } catch (nextError) {
      setError(
        nextError instanceof Error ? nextError.message : "Failed to remove",
      );
    }
  }

  function renderExpandedContent() {
    if (!expanded) return null;

    const setupMessage = getSetupMessage(
      provider.setupMethod,
      isConnected,
      supportsNativeConnect,
      t,
    );
    const nativeConnectDescription = getNativeConnectDescription(
      provider.setupMethod,
      t,
    );
    const fieldSetupDescription = getFieldSetupDescription(
      provider.setupMethod,
      t,
    );

    if (loadingConfig && hasFields) {
      return (
        <div
          ref={panelRef}
          tabIndex={-1}
          className="focus-override mx-3 space-y-3 rounded-b-lg border-x border-b px-3 py-3 outline-none"
        >
          <Skeleton className="h-12 w-full rounded-md" />
          <Skeleton className="h-12 w-full rounded-md" />
        </div>
      );
    }

    if (supportsNativeConnect && !hasFields) {
      return (
        <div
          ref={panelRef}
          tabIndex={-1}
          className="focus-override mx-3 space-y-3 rounded-b-lg border-x border-b px-3 py-3 outline-none"
        >
          {!isConnected && nativeConnectDescription ? (
            <div className="flex items-center justify-between gap-3">
              <p className="text-xs text-muted-foreground">
                {nativeConnectDescription}
              </p>
              <Button
                type="button"
                size="sm"
                onClick={() => void runNativeConnect()}
                disabled={authenticating}
                className="shrink-0"
              >
                {authenticating ? (
                  <Spinner className="size-3.5 text-current" />
                ) : null}
                {setupError ? "Retry" : "Connect"}
              </Button>
            </div>
          ) : (
            renderSetupMessage(setupMessage)
          )}
          {authenticating ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Spinner className="size-3.5 text-accent" />
              <span>{t("providers.waitingForSignIn")}</span>
            </div>
          ) : null}
          {setupOutput.length > 0 ? (
            <div className="space-y-1 rounded-md bg-muted px-3 py-2 font-mono text-xxs text-muted-foreground">
              {setupOutput.map((line) => (
                <p key={line.id}>{line.text}</p>
              ))}
            </div>
          ) : null}
          {setupError ? (
            <p className="text-xs text-danger">{setupError}</p>
          ) : null}
        </div>
      );
    }

    if (hasFields && isConnected && !preserveSetupLayout) {
      return (
        <ConnectedFieldsPanel
          panelRef={panelRef}
          fields={fields}
          fieldValueMap={fieldValueMap}
          editingKey={editingKey}
          draftValues={draftValues}
          saving={saving}
          showSavedState={showSavedState}
          error={error}
          setupMessage={setupMessage}
          onStartEdit={handleStartEdit}
          onCancelEdit={handleCancelEdit}
          onDraftChange={handleDraftChange}
          onSaveField={(field) => void handleSaveField(field)}
          onRemove={() => void handleRemove()}
        />
      );
    }

    if (hasFields) {
      return (
        <SetupFieldsPanel
          panelRef={panelRef}
          fields={fields}
          fieldValueMap={fieldValueMap}
          draftValues={draftValues}
          saving={saving}
          showSavedState={showSavedState}
          error={error}
          setupMethod={provider.setupMethod}
          setupMessage={setupMessage}
          fieldSetupDescription={fieldSetupDescription}
          isConnected={isConnected}
          onDraftChange={handleDraftChange}
          onSaveSetup={() => void handleSaveSetup()}
        />
      );
    }

    return (
      <div
        ref={panelRef}
        tabIndex={-1}
        className="focus-override mx-3 space-y-2 rounded-b-lg border-x border-b px-3 py-3 outline-none"
      >
        {renderSetupMessage(setupMessage)}
      </div>
    );
  }

  return (
    <div>
      <button
        type="button"
        onClick={handleToggle}
        aria-expanded={expanded}
        disabled={authenticating}
        className="flex w-full items-center gap-3 rounded-lg border border-border px-3 py-2.5 text-left transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:hover:bg-transparent"
      >
        <div className="flex size-6 flex-shrink-0 items-center justify-center">
          {icon || (
            <span className="text-xs font-medium text-muted-foreground">
              {formatProviderLabel(provider.id).charAt(0)}
            </span>
          )}
        </div>

        <span className="min-w-0 flex-1 text-sm">{provider.displayName}</span>

        {isConnected ? (
          <IconCheck className="size-4 flex-shrink-0 text-success" />
        ) : null}
        {!isConnected && authenticating ? (
          <Spinner className="size-3.5 flex-shrink-0 text-accent" />
        ) : null}
      </button>

      {renderExpandedContent()}
    </div>
  );
}
