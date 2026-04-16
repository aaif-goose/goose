import { invoke } from "@tauri-apps/api/core";
import type {
  DictationDownloadProgress,
  DictationProvider,
  DictationProviderStatus,
  DictationTranscribeResponse,
  MicrophonePermissionStatus,
  WhisperModelStatus,
} from "@/shared/types/dictation";
import { getClient } from "./acpConnection";

export async function getDictationConfig(): Promise<
  Record<DictationProvider, DictationProviderStatus>
> {
  const client = await getClient();
  const response = await client.goose.GooseDictationConfig({});
  return response.providers as Record<DictationProvider, DictationProviderStatus>;
}

export async function transcribeDictation(request: {
  audio: string;
  mimeType: string;
  provider: DictationProvider;
}): Promise<DictationTranscribeResponse> {
  const client = await getClient();
  return client.goose.GooseDictationTranscribe({
    audio: request.audio,
    mimeType: request.mimeType,
    provider: request.provider,
  });
}

export async function saveDictationModelSelection(
  provider: DictationProvider,
  modelId: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationModelSelect({ provider, modelId });
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
  const client = await getClient();
  const response = await client.goose.GooseDictationModelsList({});
  return response.models as unknown as WhisperModelStatus[];
}

export async function downloadDictationLocalModel(
  modelId: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationModelsDownload({ modelId });
}

export async function getDictationLocalModelDownloadProgress(
  modelId: string,
): Promise<DictationDownloadProgress | null> {
  const client = await getClient();
  const response = await client.goose.GooseDictationModelsDownloadProgress({
    modelId,
  });
  return (response.progress ?? null) as DictationDownloadProgress | null;
}

export async function cancelDictationLocalModelDownload(
  modelId: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationModelsCancel({ modelId });
}

export async function deleteDictationLocalModel(
  modelId: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationModelsDelete({ modelId });
}

export async function getMicrophonePermissionStatus(): Promise<MicrophonePermissionStatus> {
  return invoke("get_microphone_permission_status");
}

export async function requestMicrophonePermission(): Promise<MicrophonePermissionStatus> {
  return invoke("request_microphone_permission");
}
