import { useCallback, useEffect, useMemo, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  DEFAULT_AUTO_SUBMIT_PHRASES_RAW,
  DISABLED_DICTATION_PROVIDER_CONFIG_VALUE,
  normalizeDictationProvider,
  parseAutoSubmitPhrases,
} from "../lib/voiceInput";
import type { DictationProvider } from "@/shared/types/dictation";

const VOICE_INPUT_PREFERENCES_EVENT = "goose:voice-input-preferences";
const VOICE_AUTO_SUBMIT_PHRASES_PREFERENCE_KEY = "voiceAutoSubmitPhrases";
const VOICE_DICTATION_PROVIDER_PREFERENCE_KEY = "voiceDictationProvider";
const VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY =
  "voiceDictationPreferredMic";
type VoicePreferenceKey =
  | typeof VOICE_AUTO_SUBMIT_PHRASES_PREFERENCE_KEY
  | typeof VOICE_DICTATION_PROVIDER_PREFERENCE_KEY
  | typeof VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY;

type ConfigReadResult = { ok: true; value: string | null } | { ok: false };

async function readPreferenceStrings(
  keys: VoicePreferenceKey[],
): Promise<Record<VoicePreferenceKey, ConfigReadResult>> {
  const unavailable = Object.fromEntries(
    keys.map((key) => [key, { ok: false }]),
  ) as Record<VoicePreferenceKey, ConfigReadResult>;

  try {
    const client = await getClient();
    const response = await client.goose.GoosePreferencesRead({ keys });
    const values = new Map(
      response.values.map((entry) => [entry.key, entry.value]),
    );
    return Object.fromEntries(
      keys.map((key) => {
        const value = values.get(key);
        return [
          key,
          {
            ok: true,
            value: typeof value === "string" ? value : null,
          },
        ];
      }),
    ) as Record<VoicePreferenceKey, ConfigReadResult>;
  } catch {
    return unavailable;
  }
}

async function writePreferenceString(
  key: VoicePreferenceKey,
  value: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GoosePreferencesSave({ values: [{ key, value }] });
}

async function removePreferenceKey(key: VoicePreferenceKey): Promise<void> {
  const client = await getClient();
  await client.goose.GoosePreferencesRemove({ keys: [key] });
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
    const results = await readPreferenceStrings([
      VOICE_AUTO_SUBMIT_PHRASES_PREFERENCE_KEY,
      VOICE_DICTATION_PROVIDER_PREFERENCE_KEY,
      VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY,
    ]);
    const phrasesResult = results[VOICE_AUTO_SUBMIT_PHRASES_PREFERENCE_KEY];
    const providerResult = results[VOICE_DICTATION_PROVIDER_PREFERENCE_KEY];
    const micResult = results[VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY];

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
        void removePreferenceKey(VOICE_DICTATION_PROVIDER_PREFERENCE_KEY).catch(
          () => undefined,
        );
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
      void operation
        .then(() => {
          dispatchPreferencesEvent();
        })
        .catch((error: unknown) => {
          console.warn("Failed to persist voice input preferences", error);
          void syncFromConfig();
        });
    },
    [dispatchPreferencesEvent, syncFromConfig],
  );

  const setRawAutoSubmitPhrases = useCallback(
    (value: string) => {
      setRawAutoSubmitPhrasesState(value);
      persistAndBroadcast(
        writePreferenceString(VOICE_AUTO_SUBMIT_PHRASES_PREFERENCE_KEY, value),
      );
    },
    [persistAndBroadcast],
  );

  const setSelectedProvider = useCallback(
    (value: DictationProvider | null) => {
      setSelectedProviderState(value);
      setHasStoredProviderPreferenceState(true);
      persistAndBroadcast(
        writePreferenceString(
          VOICE_DICTATION_PROVIDER_PREFERENCE_KEY,
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
    persistAndBroadcast(
      removePreferenceKey(VOICE_DICTATION_PROVIDER_PREFERENCE_KEY),
    );
  }, [persistAndBroadcast]);

  const setPreferredMicrophoneId = useCallback(
    (value: string | null) => {
      setPreferredMicrophoneIdState(value);
      if (value) {
        persistAndBroadcast(
          writePreferenceString(
            VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY,
            value,
          ),
        );
      } else {
        persistAndBroadcast(
          removePreferenceKey(VOICE_DICTATION_PREFERRED_MIC_PREFERENCE_KEY),
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
