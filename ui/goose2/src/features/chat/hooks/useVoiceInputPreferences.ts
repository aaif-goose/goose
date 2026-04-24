import { useCallback, useEffect, useMemo, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  DEFAULT_AUTO_SUBMIT_PHRASES_RAW,
  DISABLED_DICTATION_PROVIDER_CONFIG_VALUE,
  VOICE_AUTO_SUBMIT_PHRASES_CONFIG_KEY,
  VOICE_DICTATION_PREFERRED_MIC_CONFIG_KEY,
  VOICE_DICTATION_PROVIDER_CONFIG_KEY,
  normalizeDictationProvider,
  parseAutoSubmitPhrases,
} from "../lib/voiceInput";
import type { DictationProvider } from "@/shared/types/dictation";

const VOICE_INPUT_PREFERENCES_EVENT = "goose:voice-input-preferences";

type ConfigReadResult = { ok: true; value: string | null } | { ok: false };

async function readConfigString(key: string): Promise<ConfigReadResult> {
  try {
    const client = await getClient();
    const response = await client.goose.GooseConfigRead({ key });
    return {
      ok: true,
      value: typeof response.value === "string" ? response.value : null,
    };
  } catch {
    return { ok: false };
  }
}

async function writeConfigString(key: string, value: string): Promise<void> {
  try {
    const client = await getClient();
    await client.goose.GooseConfigUpsert({ key, value });
  } catch {
    // goose config may be unavailable
  }
}

async function removeConfigKey(key: string): Promise<void> {
  try {
    const client = await getClient();
    await client.goose.GooseConfigRemove({ key });
  } catch {
    // goose config may be unavailable
  }
}

export function useVoiceInputPreferences() {
  const [rawAutoSubmitPhrases, setRawAutoSubmitPhrasesState] = useState<string>(
    DEFAULT_AUTO_SUBMIT_PHRASES_RAW,
  );
  const [selectedProvider, setSelectedProviderState] =
    useState<DictationProvider | null>(null);
  const [hasStoredProviderPreference, setHasStoredProviderPreferenceState] =
    useState<boolean>(false);
  const [preferredMicrophoneId, setPreferredMicrophoneIdState] = useState<
    string | null
  >(null);
  // Flips true after the first syncFromConfig completes so consumers can
  // distinguish "no stored preference" from "the ACP round-trip hasn't
  // finished yet." Without this, a consumer that auto-writes a default when
  // hasStoredProviderPreference is false can race ahead and overwrite the
  // user's saved choice before it loads.
  const [isHydrated, setIsHydrated] = useState(false);

  const syncFromConfig = useCallback(async () => {
    const [phrasesResult, providerResult, micResult] = await Promise.all([
      readConfigString(VOICE_AUTO_SUBMIT_PHRASES_CONFIG_KEY),
      readConfigString(VOICE_DICTATION_PROVIDER_CONFIG_KEY),
      readConfigString(VOICE_DICTATION_PREFERRED_MIC_CONFIG_KEY),
    ]);

    if (phrasesResult.ok) {
      setRawAutoSubmitPhrasesState(
        phrasesResult.value ?? DEFAULT_AUTO_SUBMIT_PHRASES_RAW,
      );
    }

    if (!providerResult.ok) {
      if (micResult.ok) {
        setPreferredMicrophoneIdState(micResult.value);
      }
      return;
    }

    if (providerResult.value === DISABLED_DICTATION_PROVIDER_CONFIG_VALUE) {
      setSelectedProviderState(null);
      setHasStoredProviderPreferenceState(true);
    } else if (providerResult.value != null) {
      const normalized = normalizeDictationProvider(providerResult.value);
      if (normalized !== null) {
        setSelectedProviderState(normalized);
        setHasStoredProviderPreferenceState(true);
      } else {
        // Stored value isn't a recognized provider (stale from an older
        // build, typo, etc.). Treat as no preference — don't pin the user
        // to voice-off — and clear the config key so future boots fall
        // through to the default cleanly.
        setSelectedProviderState(null);
        setHasStoredProviderPreferenceState(false);
        void removeConfigKey(VOICE_DICTATION_PROVIDER_CONFIG_KEY);
      }
    } else {
      setSelectedProviderState(null);
      setHasStoredProviderPreferenceState(false);
    }

    if (micResult.ok) {
      setPreferredMicrophoneIdState(micResult.value);
    }
    setIsHydrated(true);
  }, []);

  useEffect(() => {
    void syncFromConfig();
    const handler = () => {
      void syncFromConfig();
    };
    window.addEventListener(
      VOICE_INPUT_PREFERENCES_EVENT,
      handler as EventListener,
    );
    return () => {
      window.removeEventListener(
        VOICE_INPUT_PREFERENCES_EVENT,
        handler as EventListener,
      );
    };
  }, [syncFromConfig]);

  const dispatchPreferencesEvent = useCallback(() => {
    window.dispatchEvent(new Event(VOICE_INPUT_PREFERENCES_EVENT));
  }, []);

  const persistAndBroadcast = useCallback(
    (operation: Promise<void>) => {
      void operation.finally(() => {
        dispatchPreferencesEvent();
      });
    },
    [dispatchPreferencesEvent],
  );

  const setRawAutoSubmitPhrases = useCallback(
    (value: string) => {
      setRawAutoSubmitPhrasesState(value);
      persistAndBroadcast(
        writeConfigString(VOICE_AUTO_SUBMIT_PHRASES_CONFIG_KEY, value),
      );
    },
    [persistAndBroadcast],
  );

  const setSelectedProvider = useCallback(
    (value: DictationProvider | null) => {
      setSelectedProviderState(value);
      setHasStoredProviderPreferenceState(true);
      persistAndBroadcast(
        writeConfigString(
          VOICE_DICTATION_PROVIDER_CONFIG_KEY,
          value ?? DISABLED_DICTATION_PROVIDER_CONFIG_VALUE,
        ),
      );
    },
    [persistAndBroadcast],
  );

  // Remove the stored preference entirely, so the user falls through to the
  // default provider on next boot. Distinct from setSelectedProvider(null),
  // which pins the user to "voice off" via a sentinel value.
  const clearSelectedProvider = useCallback(() => {
    setSelectedProviderState(null);
    setHasStoredProviderPreferenceState(false);
    persistAndBroadcast(removeConfigKey(VOICE_DICTATION_PROVIDER_CONFIG_KEY));
  }, [persistAndBroadcast]);

  const setPreferredMicrophoneId = useCallback(
    (value: string | null) => {
      setPreferredMicrophoneIdState(value);
      if (value) {
        persistAndBroadcast(
          writeConfigString(VOICE_DICTATION_PREFERRED_MIC_CONFIG_KEY, value),
        );
      } else {
        persistAndBroadcast(
          removeConfigKey(VOICE_DICTATION_PREFERRED_MIC_CONFIG_KEY),
        );
      }
    },
    [persistAndBroadcast],
  );

  const autoSubmitPhrases = useMemo(
    () => parseAutoSubmitPhrases(rawAutoSubmitPhrases),
    [rawAutoSubmitPhrases],
  );

  return {
    autoSubmitPhrases,
    clearSelectedProvider,
    hasStoredProviderPreference,
    isHydrated,
    preferredMicrophoneId,
    rawAutoSubmitPhrases,
    selectedProvider,
    setPreferredMicrophoneId,
    setRawAutoSubmitPhrases,
    setSelectedProvider,
  };
}
