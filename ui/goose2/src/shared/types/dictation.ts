export type {
  DictationModelOption,
  DictationProviderStatusEntry as DictationProviderStatus,
  DictationTranscribeResponse,
  DictationLocalModelStatus as WhisperModelStatus,
  DictationDownloadProgress,
} from "@aaif/goose-sdk";

export type DictationProvider = "openai" | "groq" | "elevenlabs" | "local";

export type MicrophonePermissionStatus =
  | "not_determined"
  | "authorized"
  | "denied"
  | "restricted"
  | "unsupported";
