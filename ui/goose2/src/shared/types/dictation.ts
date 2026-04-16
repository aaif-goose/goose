export type DictationProvider = "openai" | "groq" | "elevenlabs" | "local";

export interface DictationModelOption {
  id: string;
  label: string;
  description: string;
}

export interface DictationProviderStatus {
  configured: boolean;
  host?: string | null;
  description: string;
  usesProviderConfig: boolean;
  settingsPath?: string | null;
  configKey?: string | null;
  modelConfigKey?: string | null;
  defaultModel?: string | null;
  selectedModel?: string | null;
  availableModels: DictationModelOption[];
}

export interface DictationTranscribeResponse {
  text: string;
}

export type MicrophonePermissionStatus =
  | "not_determined"
  | "authorized"
  | "denied"
  | "restricted"
  | "unsupported";

export interface WhisperModelStatus {
  id: string;
  sizeMb: number;
  description: string;
  downloaded: boolean;
  downloadInProgress: boolean;
}

export interface DictationDownloadProgress {
  bytesDownloaded: number;
  totalBytes: number;
  progressPercent: number;
  status: string;
  error?: string | null;
}
