import { useCallback, useEffect, useRef, useState } from "react";
import { getDictationConfig } from "@/shared/api/dictation";
import type { DictationProviderStatus } from "@/shared/types/dictation";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { useDictationRecorder } from "./useDictationRecorder";
import { useVoiceInputPreferences } from "./useVoiceInputPreferences";
import {
  appendTranscribedText,
  getAutoSubmitMatch,
  getDefaultDictationProvider,
  VOICE_DICTATION_CONFIG_EVENT,
} from "../lib/voiceInput";

interface UseVoiceDictationOptions {
  text: string;
  setText: (value: string) => void;
  attachments: ChatAttachmentDraft[];
  clearAttachments: () => void;
  selectedPersonaId: string | null;
  onSend: (
    text: string,
    personaId?: string,
    attachments?: ChatAttachmentDraft[],
  ) => void;
  resetTextarea: () => void;
}

export function useVoiceDictation({
  text,
  setText,
  attachments,
  clearAttachments,
  selectedPersonaId,
  onSend,
  resetTextarea,
}: UseVoiceDictationOptions) {
  const voicePrefs = useVoiceInputPreferences();
  const [providerStatuses, setProviderStatuses] = useState<
    Partial<Record<string, DictationProviderStatus>>
  >({});

  const fetchDictationConfig = useCallback(() => {
    getDictationConfig()
      .then(setProviderStatuses)
      .catch(() => {});
  }, []);

  useEffect(() => {
    fetchDictationConfig();
    window.addEventListener(VOICE_DICTATION_CONFIG_EVENT, fetchDictationConfig);
    return () =>
      window.removeEventListener(
        VOICE_DICTATION_CONFIG_EVENT,
        fetchDictationConfig,
      );
  }, [fetchDictationConfig]);

  const activeVoiceProvider =
    voicePrefs.selectedProvider ??
    (voicePrefs.hasStoredProviderPreference
      ? null
      : getDefaultDictationProvider(providerStatuses));

  const providerConfigured =
    activeVoiceProvider != null &&
    providerStatuses[activeVoiceProvider]?.configured === true;

  const stopRecordingRef = useRef<
    (options?: { flushPending?: boolean }) => void
  >(() => {});

  const handleTranscription = useCallback(
    (fragment: string) => {
      const match = getAutoSubmitMatch(fragment, voicePrefs.autoSubmitPhrases);
      if (match) {
        const merged = appendTranscribedText(text, match.textWithoutPhrase);
        if (merged.trim()) {
          stopRecordingRef.current({ flushPending: false });
          onSend(
            merged.trim(),
            selectedPersonaId ?? undefined,
            attachments.length > 0 ? attachments : undefined,
          );
          setText("");
          clearAttachments();
          resetTextarea();
        }
      } else {
        const merged = appendTranscribedText(text, fragment);
        setText(merged);
      }
    },
    [
      attachments,
      clearAttachments,
      onSend,
      resetTextarea,
      selectedPersonaId,
      setText,
      text,
      voicePrefs.autoSubmitPhrases,
    ],
  );

  const handleVoiceError = useCallback((_message: string) => {}, []);

  const dictation = useDictationRecorder({
    provider: activeVoiceProvider,
    providerConfigured,
    preferredMicrophoneId: voicePrefs.preferredMicrophoneId,
    onError: handleVoiceError,
    onTranscription: handleTranscription,
  });
  stopRecordingRef.current = dictation.stopRecording;

  return dictation;
}
