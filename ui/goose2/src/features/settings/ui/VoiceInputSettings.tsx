import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  deleteDictationProviderSecret,
  getDictationConfig,
  saveDictationModelSelection,
  saveDictationProviderSecret,
} from "@/shared/api/dictation";
import {
  notifyVoiceDictationConfigChanged,
  getDefaultDictationProvider,
} from "@/features/chat/lib/voiceInput";
import { useVoiceInputPreferences } from "@/features/chat/hooks/useVoiceInputPreferences";
import type {
  DictationProvider,
  DictationProviderStatus,
} from "@/shared/types/dictation";
import { useAudioDevices } from "@/shared/ui/ai-elements/mic-selector";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";

const DISABLED_PROVIDER = "__disabled__";

export function VoiceInputSettings() {
  const { t } = useTranslation(["settings", "chat", "common"]);
  const {
    hasStoredProviderPreference,
    preferredMicrophoneId,
    rawAutoSubmitPhrases,
    selectedProvider,
    setPreferredMicrophoneId,
    setRawAutoSubmitPhrases,
    setSelectedProvider,
  } = useVoiceInputPreferences();
  const [providerStatuses, setProviderStatuses] = useState<
    Record<DictationProvider, DictationProviderStatus>
  >({} as Record<DictationProvider, DictationProviderStatus>);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [isEditingApiKey, setIsEditingApiKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const {
    devices,
    error: devicesError,
    hasPermission,
    loadDevices,
    loading: loadingDevices,
  } = useAudioDevices();
  const isMicrophoneSupported =
    typeof navigator !== "undefined" && !!navigator.mediaDevices;
  const permissionStatus = hasPermission ? "authorized" : "not_determined";
  const requestPermission = loadDevices;

  const refreshConfig = useCallback(async () => {
    const nextConfig = await getDictationConfig();
    setProviderStatuses(nextConfig);

    if (!hasStoredProviderPreference) {
      const defaultProvider = getDefaultDictationProvider(nextConfig);
      if (defaultProvider) {
        setSelectedProvider(defaultProvider);
      }
      return;
    }

    if (!selectedProvider) {
      return;
    }

    if (!nextConfig[selectedProvider]) {
      setSelectedProvider(null);
    }
  }, [hasStoredProviderPreference, selectedProvider, setSelectedProvider]);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      setError(null);

      try {
        await refreshConfig();
      } catch (caughtError) {
        setError(
          caughtError instanceof Error
            ? caughtError.message
            : t("general.voiceInput.loadError"),
        );
      } finally {
        setLoading(false);
      }
    };

    void load();
  }, [refreshConfig, t]);

  const selectedStatus = selectedProvider
    ? providerStatuses[selectedProvider]
    : null;

  const providerOptions = useMemo(
    () =>
      Object.entries(providerStatuses) as Array<
        [DictationProvider, DictationProviderStatus]
      >,
    [providerStatuses],
  );

  const currentModelValue =
    selectedStatus?.selectedModel ?? selectedStatus?.defaultModel ?? "";

  const saveApiKey = useCallback(async () => {
    if (!selectedProvider) {
      return;
    }

    setError(null);
    try {
      await saveDictationProviderSecret(
        selectedProvider,
        apiKeyInput,
        selectedStatus?.configKey ?? undefined,
      );
      setApiKeyInput("");
      setIsEditingApiKey(false);
      await refreshConfig();
      notifyVoiceDictationConfigChanged();
    } catch (caughtError) {
      setError(
        caughtError instanceof Error
          ? caughtError.message
          : t("general.voiceInput.saveError"),
      );
    }
  }, [apiKeyInput, refreshConfig, selectedProvider, selectedStatus, t]);

  const removeApiKey = useCallback(async () => {
    if (!selectedProvider) {
      return;
    }

    setError(null);
    try {
      await deleteDictationProviderSecret(
        selectedProvider,
        selectedStatus?.configKey ?? undefined,
      );
      setApiKeyInput("");
      setIsEditingApiKey(false);
      await refreshConfig();
      notifyVoiceDictationConfigChanged();
    } catch (caughtError) {
      setError(
        caughtError instanceof Error
          ? caughtError.message
          : t("general.voiceInput.deleteError"),
      );
    }
  }, [refreshConfig, selectedProvider, selectedStatus, t]);

  const handleModelChange = useCallback(
    async (modelId: string) => {
      if (!selectedProvider) {
        return;
      }

      setError(null);
      try {
        await saveDictationModelSelection(selectedProvider, modelId);
        await refreshConfig();
        notifyVoiceDictationConfigChanged();
      } catch (caughtError) {
        setError(
          caughtError instanceof Error
            ? caughtError.message
            : t("general.voiceInput.saveError"),
        );
      }
    },
    [refreshConfig, selectedProvider, t],
  );

  const selectedMicrophoneLabel = useMemo(() => {
    if (!preferredMicrophoneId) {
      return t("general.voiceInput.systemMicrophone");
    }

    return (
      devices.find((device) => device.deviceId === preferredMicrophoneId)
        ?.label || t("general.voiceInput.systemMicrophone")
    );
  }, [devices, preferredMicrophoneId, t]);

  if (loading) {
    return (
      <div className="space-y-3">
        <h4 className="text-sm font-semibold">
          {t("general.voiceInput.label")}
        </h4>
        <p className="text-xs text-muted-foreground">
          {t("common:labels.loading")}
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div>
        <h4 className="text-sm font-semibold">
          {t("general.voiceInput.label")}
        </h4>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("general.voiceInput.description")}
        </p>
      </div>

      <div className="space-y-2 rounded-lg border border-border px-3 py-3">
        <p className="text-xs font-medium text-foreground">
          {t("general.voiceInput.providerLabel")}
        </p>
        <Select
          value={selectedProvider ?? DISABLED_PROVIDER}
          onValueChange={(value) =>
            setSelectedProvider(
              value === DISABLED_PROVIDER ? null : (value as DictationProvider),
            )
          }
        >
          <SelectTrigger className="w-full max-w-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DISABLED_PROVIDER}>
              {t("general.voiceInput.disabled")}
            </SelectItem>
            {providerOptions.map(([providerId, status]) => (
              <SelectItem key={providerId} value={providerId}>
                {t(`general.voiceInput.providers.${providerId}`)}
                {!status.configured
                  ? ` ${t("general.voiceInput.notConfiguredSuffix")}`
                  : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {selectedStatus ? (
        <>
          {!selectedStatus.usesProviderConfig &&
          selectedProvider !== "local" ? (
            <div className="space-y-3 rounded-lg border border-border px-3 py-3">
              {isEditingApiKey ? (
                <>
                  <div>
                    <p className="text-xs font-medium text-foreground">
                      {t("general.voiceInput.apiKeyLabel")}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {t("general.voiceInput.apiKeyDescription")}
                    </p>
                  </div>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      type="password"
                      value={apiKeyInput}
                      onChange={(event) => setApiKeyInput(event.target.value)}
                      placeholder={t("general.voiceInput.apiKeyPlaceholder")}
                      className="max-w-sm"
                    />
                    <div className="flex gap-2">
                      <Button
                        type="button"
                        size="sm"
                        onClick={() => void saveApiKey()}
                      >
                        {t("common:actions.save")}
                      </Button>
                      <Button
                        type="button"
                        variant="outline-flat"
                        size="sm"
                        onClick={() => {
                          setApiKeyInput("");
                          setIsEditingApiKey(false);
                        }}
                      >
                        {t("common:actions.cancel")}
                      </Button>
                    </div>
                  </div>
                </>
              ) : (
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-xs font-medium text-foreground">
                      {t("general.voiceInput.apiKeyLabel")}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {selectedStatus.configured
                        ? t("general.voiceInput.apiKeyConfigured")
                        : t("general.voiceInput.apiKeyDescription")}
                    </p>
                  </div>
                  <div className="flex gap-2 flex-shrink-0">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline-flat"
                      onClick={() => setIsEditingApiKey(true)}
                    >
                      {selectedStatus.configured
                        ? t("general.voiceInput.updateApiKey")
                        : t("general.voiceInput.addApiKey")}
                    </Button>
                    {selectedStatus.configured ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-destructive hover:text-destructive"
                        onClick={() => void removeApiKey()}
                      >
                        {t("general.voiceInput.removeApiKey")}
                      </Button>
                    ) : null}
                  </div>
                </div>
              )}
            </div>
          ) : null}

          {selectedProvider === "local" ? (
            <div className="rounded-lg border border-border px-3 py-3">
              <p className="text-xs font-medium text-foreground">
                {t("general.voiceInput.localModelLabel")}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("general.voiceInput.localModelUnavailable")}
              </p>
            </div>
          ) : (selectedStatus.availableModels ?? []).length > 0 ? (
            <div className="space-y-2 rounded-lg border border-border px-3 py-3">
              <p className="text-xs font-medium text-foreground">
                {t("general.voiceInput.modelLabel")}
              </p>
              <Select
                value={currentModelValue}
                onValueChange={(value) => void handleModelChange(value)}
              >
                <SelectTrigger className="w-full max-w-sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(selectedStatus.availableModels ?? []).map((model) => (
                    <SelectItem key={model.id} value={model.id}>
                      {model.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                {(selectedStatus.availableModels ?? []).find(
                  (model) => model.id === currentModelValue,
                )?.description ?? ""}
              </p>
            </div>
          ) : null}
        </>
      ) : null}

      <div className="space-y-2 rounded-lg border border-border px-3 py-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-xs font-medium text-foreground">
              {t("general.voiceInput.microphoneLabel")}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              {isMicrophoneSupported
                ? t("general.voiceInput.microphoneDescription")
                : t("general.voiceInput.microphoneUnavailable")}
            </p>
          </div>
          {isMicrophoneSupported && !hasPermission ? (
            <Button
              type="button"
              size="sm"
              variant="outline-flat"
              disabled={loadingDevices}
              onClick={() => void requestPermission()}
            >
              {t("general.voiceInput.grantMicrophone")}
            </Button>
          ) : null}
        </div>

        {!devicesError &&
        !hasPermission &&
        permissionStatus === "not_determined" ? (
          <p className="text-xs text-muted-foreground">
            {t("general.voiceInput.microphoneAccessPrompt")}
          </p>
        ) : null}

        {devicesError ? (
          <p className="text-xs text-muted-foreground">{devicesError}</p>
        ) : null}

        {isMicrophoneSupported && hasPermission ? (
          <Select
            value={preferredMicrophoneId ?? DISABLED_PROVIDER}
            onValueChange={(value) =>
              setPreferredMicrophoneId(
                value === DISABLED_PROVIDER ? null : value,
              )
            }
          >
            <SelectTrigger className="w-full max-w-sm">
              <SelectValue>{selectedMicrophoneLabel}</SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DISABLED_PROVIDER}>
                {t("general.voiceInput.systemMicrophone")}
              </SelectItem>
              {devices.map((device) => (
                <SelectItem key={device.deviceId} value={device.deviceId}>
                  {device.label || t("general.voiceInput.unknownMicrophone")}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
      </div>

      <div className="space-y-2 rounded-lg border border-border px-3 py-3">
        <label
          htmlFor="voice-auto-submit-phrases"
          className="text-xs font-medium text-foreground"
        >
          {t("general.voiceInput.autoSubmitLabel")}
        </label>
        <p className="text-xs text-muted-foreground">
          {t("general.voiceInput.autoSubmitDescription")}
        </p>
        <Input
          id="voice-auto-submit-phrases"
          type="text"
          value={rawAutoSubmitPhrases}
          onChange={(event) => setRawAutoSubmitPhrases(event.target.value)}
          placeholder={t("general.voiceInput.placeholder")}
          className="max-w-sm"
        />
      </div>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}
    </div>
  );
}
