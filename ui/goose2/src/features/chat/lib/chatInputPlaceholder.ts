const DEFAULT_LABEL_SUFFIX = " (Default)";

export function getChatInputAgentLabel(
  personaDisplayName: string | undefined,
  providerDisplayName: string,
): string {
  const displayName = personaDisplayName ?? providerDisplayName;
  return displayName.endsWith(DEFAULT_LABEL_SUFFIX)
    ? displayName.slice(0, -DEFAULT_LABEL_SUFFIX.length)
    : displayName;
}

export function getChatInputPlaceholder(
  t: (key: string, options?: { agent: string }) => string,
  agent: string,
  isRecording: boolean,
  isTranscribing: boolean,
): string {
  if (isRecording) return t("toolbar.voiceInputRecording");
  if (isTranscribing) return t("toolbar.voiceInputTranscribing");
  return t("input.placeholder", { agent });
}
