import { invoke } from "@tauri-apps/api/core";
import type {
  DictationDownloadProgress,
  DictationProvider,
  DictationProviderStatus,
  DictationTranscribeResponse,
  MicrophonePermissionStatus,
  WhisperModelStatus,
} from "@/shared/types/dictation";

export async function getDictationConfig(): Promise<
  Record<DictationProvider, DictationProviderStatus>
> {
  return invoke("get_dictation_config");
}

export async function transcribeDictation(request: {
  audio: string;
  mimeType: string;
  provider: DictationProvider;
}): Promise<DictationTranscribeResponse> {
  return invoke("transcribe_dictation", {
    request: {
      audio: request.audio,
      mimeType: request.mimeType,
      provider: request.provider,
    },
  });
}

export async function saveDictationModelSelection(
  provider: DictationProvider,
  modelId: string,
): Promise<void> {
  return invoke("save_dictation_model_selection", { provider, modelId });
}

export async function saveDictationProviderSecret(
  _provider: DictationProvider,
  value: string,
  configKey?: string,
): Promise<void> {
  if (!configKey) {
    throw new Error("No config key for this provider");
  }
  return invoke("save_provider_field", { key: configKey, value });
}

export async function deleteDictationProviderSecret(
  provider: DictationProvider,
  _configKey?: string,
): Promise<void> {
  const providerIdMap: Record<string, string> = {
    groq: "dictation_groq",
    elevenlabs: "dictation_elevenlabs",
  };
  const providerId = providerIdMap[provider];
  if (!providerId) {
    throw new Error("Cannot delete secrets for this provider");
  }
  return invoke("delete_provider_config", { providerId });
}

export async function listDictationLocalModels(): Promise<
  WhisperModelStatus[]
> {
  return invoke("list_dictation_local_models");
}

export async function downloadDictationLocalModel(
  modelId: string,
): Promise<void> {
  return invoke("download_dictation_local_model", { modelId });
}

export async function getDictationLocalModelDownloadProgress(
  modelId: string,
): Promise<DictationDownloadProgress | null> {
  return invoke("get_dictation_local_model_download_progress", { modelId });
}

export async function cancelDictationLocalModelDownload(
  modelId: string,
): Promise<void> {
  return invoke("cancel_dictation_local_model_download", { modelId });
}

export async function deleteDictationLocalModel(
  modelId: string,
): Promise<void> {
  return invoke("delete_dictation_local_model", { modelId });
}

export async function getMicrophonePermissionStatus(): Promise<MicrophonePermissionStatus> {
  return invoke("get_microphone_permission_status");
}

export async function requestMicrophonePermission(): Promise<MicrophonePermissionStatus> {
  return invoke("request_microphone_permission");
}
