import { useCallback, useEffect, useRef, useState } from "react";
import { getDictationConfig } from "@/shared/api/dictation";
import { isPromiseLike } from "@/shared/lib/isPromiseLike";
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
  ) => boolean | Promise<boolean>;
  resetTextarea: () => void;
  /**
   * When true, auto-submit on trigger phrase will NOT call `onSend`.
   * Instead, the trigger phrase is stripped and the remaining transcription
   * is left in the textarea for the user to review and send manually.
   * Caller should set this to match `ChatInput`'s own send guards
   * (queued-message lockout, outer `disabled` state, etc.) so voice
   * auto-submit can't bypass the UI's protection against extra sends
   * during an active run.
   */
  isSendLocked?: boolean;
}

export function useVoiceDictation({
  text,
  setText,
  attachments,
  clearAttachments,
  selectedPersonaId,
  onSend,
  resetTextarea,
  isSendLocked = false,
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

  // Treat the stored preference as valid only when it actually appears in
  // `providerStatuses`. If the stored value points at a provider that's been
  // feature-flagged off or removed, fall through to the default so voice
  // input isn't silently disabled. The explicit "off" state
  // (`hasStoredProviderPreference && selectedProvider == null`) is preserved.
  const storedProviderIsPresent =
    voicePrefs.selectedProvider != null &&
    providerStatuses[voicePrefs.selectedProvider] !== undefined;

  const activeVoiceProvider = !voicePrefs.isHydrated
    ? null
    : storedProviderIsPresent
      ? voicePrefs.selectedProvider
      : voicePrefs.hasStoredProviderPreference &&
          voicePrefs.selectedProvider == null
        ? null
        : getDefaultDictationProvider(providerStatuses);

  // If a stored preference points at a provider that's no longer in
  // providerStatuses (feature-flagged off, removed), clear it so next boot
  // falls through to the default cleanly instead of re-detecting the stale
  // value every session.
  useEffect(() => {
    if (
      voicePrefs.selectedProvider != null &&
      Object.keys(providerStatuses).length > 0 &&
      providerStatuses[voicePrefs.selectedProvider] === undefined
    ) {
      voicePrefs.clearSelectedProvider();
    }
  }, [providerStatuses, voicePrefs]);

  const providerConfigured =
    activeVoiceProvider != null &&
    providerStatuses[activeVoiceProvider]?.configured === true;

  const stopRecordingRef = useRef<
    (options?: { flushPending?: boolean }) => void
  >(() => {});

  // Mirror `text` in a ref so `handleTranscription` always sees the latest
  // value, even when `useDictationRecorder` fires multiple callbacks in the
  // same tick before React has applied the first setText. Without this, two
  // concurrent callbacks would both read a stale `text` from closure and the
  // second would overwrite the first fragment, dropping dictated words.
  //
  // Assign during render (not in a post-render `useEffect`) so there is no
  // commit-window race: if the user types a character in the textarea and a
  // transcription callback resolves before the effect runs, the callback
  // would otherwise read the previous `text` and clobber the user's edit.
  // Writing to `ref.current` during render is explicitly supported by React
  // (see `providerRef.current = provider;` in `useDictationRecorder.ts`).
  const textRef = useRef(text);
  textRef.current = text;

  const handleTranscription = useCallback(
    (fragment: string) => {
      const latest = textRef.current;
      const match = getAutoSubmitMatch(fragment, voicePrefs.autoSubmitPhrases);
      if (match) {
        const merged = appendTranscribedText(latest, match.textWithoutPhrase);
        if (!merged.trim()) {
          return;
        }
        stopRecordingRef.current({ flushPending: false });
        if (isSendLocked) {
          // Parent UI is blocking sends (queued message, disabled, etc.).
          // Strip the trigger phrase and leave the transcription in the
          // textarea so the user can send it manually when the lock clears.
          setText(merged);
          textRef.current = merged;
          return;
        }
        const sendResult = onSend(
          merged.trim(),
          selectedPersonaId ?? undefined,
          attachments.length > 0 ? attachments : undefined,
        );
        if (isPromiseLike<boolean>(sendResult)) {
          void sendResult
            .then((accepted) => {
              if (accepted === false) {
                setText(merged);
                textRef.current = merged;
                return;
              }
              setText("");
              textRef.current = "";
              clearAttachments();
              resetTextarea();
            })
            .catch(() => {
              setText(merged);
              textRef.current = merged;
            });
          return;
        }
        if (sendResult === false) {
          setText(merged);
          textRef.current = merged;
          return;
        }
        setText("");
        textRef.current = "";
        clearAttachments();
        resetTextarea();
      } else {
        const merged = appendTranscribedText(latest, fragment);
        setText(merged);
        textRef.current = merged;
      }
    },
    [
      attachments,
      clearAttachments,
      isSendLocked,
      onSend,
      resetTextarea,
      selectedPersonaId,
      setText,
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
