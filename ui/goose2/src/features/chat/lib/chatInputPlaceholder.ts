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
