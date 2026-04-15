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
  url: string;
  description: string;
  downloaded: boolean;
  recommended: boolean;
}

export interface DictationDownloadProgress {
  modelId: string;
  status: string;
  bytesDownloaded: number;
  totalBytes: number;
  progressPercent: number;
  speedBps?: number | null;
  etaSeconds?: number | null;
  error?: string | null;
}
