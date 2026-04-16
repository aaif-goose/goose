import { useCallback, useEffect, useMemo, useState } from "react";
import {
  DISABLED_DICTATION_PROVIDER_STORAGE_VALUE,
  DEFAULT_AUTO_SUBMIT_PHRASES_RAW,
  VOICE_AUTO_SUBMIT_PHRASES_STORAGE_KEY,
  VOICE_DICTATION_PREFERRED_MIC_STORAGE_KEY,
  VOICE_DICTATION_PROVIDER_STORAGE_KEY,
  normalizeDictationProvider,
  parseAutoSubmitPhrases,
} from "../lib/voiceInput";
import type { DictationProvider } from "@/shared/types/dictation";

const VOICE_INPUT_PREFERENCES_EVENT = "goose:voice-input-preferences";

function readStoredAutoSubmitPhrases() {
  try {
    return (
      window.localStorage.getItem(VOICE_AUTO_SUBMIT_PHRASES_STORAGE_KEY) ??
      DEFAULT_AUTO_SUBMIT_PHRASES_RAW
    );
  } catch {
    return DEFAULT_AUTO_SUBMIT_PHRASES_RAW;
  }
}

function readStoredDictationProvider(): DictationProvider | null {
  try {
    const storedValue = window.localStorage.getItem(
      VOICE_DICTATION_PROVIDER_STORAGE_KEY,
    );

    if (storedValue === DISABLED_DICTATION_PROVIDER_STORAGE_VALUE) {
      return null;
    }

    return normalizeDictationProvider(storedValue);
  } catch {
    return null;
  }
}

function readHasStoredDictationProviderPreference() {
  try {
    return (
      window.localStorage.getItem(VOICE_DICTATION_PROVIDER_STORAGE_KEY) !== null
    );
  } catch {
    return false;
  }
}

function readStoredPreferredMicrophoneId() {
  try {
    return window.localStorage.getItem(
      VOICE_DICTATION_PREFERRED_MIC_STORAGE_KEY,
    );
  } catch {
    return null;
  }
}

export function useVoiceInputPreferences() {
  const [rawAutoSubmitPhrases, setRawAutoSubmitPhrasesState] = useState(
    readStoredAutoSubmitPhrases,
  );
  const [selectedProvider, setSelectedProviderState] = useState(
    readStoredDictationProvider,
  );
  const [hasStoredProviderPreference, setHasStoredProviderPreferenceState] =
    useState(readHasStoredDictationProviderPreference);
  const [preferredMicrophoneId, setPreferredMicrophoneIdState] = useState(
    readStoredPreferredMicrophoneId,
  );

  useEffect(() => {
    const syncFromStorage = () => {
      setRawAutoSubmitPhrasesState(readStoredAutoSubmitPhrases());
      setSelectedProviderState(readStoredDictationProvider());
      setHasStoredProviderPreferenceState(
        readHasStoredDictationProviderPreference(),
      );
      setPreferredMicrophoneIdState(readStoredPreferredMicrophoneId());
    };

    window.addEventListener("storage", syncFromStorage);
    window.addEventListener(
      VOICE_INPUT_PREFERENCES_EVENT,
      syncFromStorage as EventListener,
    );

    return () => {
      window.removeEventListener("storage", syncFromStorage);
      window.removeEventListener(
        VOICE_INPUT_PREFERENCES_EVENT,
        syncFromStorage as EventListener,
      );
    };
  }, []);

  const setRawAutoSubmitPhrases = useCallback((value: string) => {
    setRawAutoSubmitPhrasesState(value);

    try {
      window.localStorage.setItem(VOICE_AUTO_SUBMIT_PHRASES_STORAGE_KEY, value);
      window.dispatchEvent(new Event(VOICE_INPUT_PREFERENCES_EVENT));
    } catch {
      // localStorage may be unavailable
    }
  }, []);

  const setSelectedProvider = useCallback((value: DictationProvider | null) => {
    setSelectedProviderState(value);
    setHasStoredProviderPreferenceState(true);

    try {
      window.localStorage.setItem(
        VOICE_DICTATION_PROVIDER_STORAGE_KEY,
        value ?? DISABLED_DICTATION_PROVIDER_STORAGE_VALUE,
      );
      window.dispatchEvent(new Event(VOICE_INPUT_PREFERENCES_EVENT));
    } catch {
      // localStorage may be unavailable
    }
  }, []);

  // Remove the stored preference entirely, so the user falls through to the
  // default provider on next boot. Distinct from setSelectedProvider(null),
  // which explicitly pins the user to "voice off" via a sentinel value.
  const clearSelectedProvider = useCallback(() => {
    setSelectedProviderState(null);
    setHasStoredProviderPreferenceState(false);

    try {
      window.localStorage.removeItem(VOICE_DICTATION_PROVIDER_STORAGE_KEY);
      window.dispatchEvent(new Event(VOICE_INPUT_PREFERENCES_EVENT));
    } catch {
      // localStorage may be unavailable
    }
  }, []);

  const setPreferredMicrophoneId = useCallback((value: string | null) => {
    setPreferredMicrophoneIdState(value);

    try {
      if (value) {
        window.localStorage.setItem(
          VOICE_DICTATION_PREFERRED_MIC_STORAGE_KEY,
          value,
        );
      } else {
        window.localStorage.removeItem(
          VOICE_DICTATION_PREFERRED_MIC_STORAGE_KEY,
        );
      }
      window.dispatchEvent(new Event(VOICE_INPUT_PREFERENCES_EVENT));
    } catch {
      // localStorage may be unavailable
    }
  }, []);

  const autoSubmitPhrases = useMemo(
    () => parseAutoSubmitPhrases(rawAutoSubmitPhrases),
    [rawAutoSubmitPhrases],
  );

  return {
    autoSubmitPhrases,
    clearSelectedProvider,
    hasStoredProviderPreference,
    preferredMicrophoneId,
    rawAutoSubmitPhrases,
    selectedProvider,
    setPreferredMicrophoneId,
    setRawAutoSubmitPhrases,
    setSelectedProvider,
  };
}
