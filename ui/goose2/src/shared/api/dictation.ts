import type {
  DictationDownloadProgress,
  DictationProvider,
  DictationProviderStatus,
  DictationTranscribeResponse,
  WhisperModelStatus,
} from "@/shared/types/dictation";
import { getClient } from "./acpConnection";

export async function getDictationConfig(): Promise<
  Record<DictationProvider, DictationProviderStatus>
> {
  const client = await getClient();
  const response = await client.goose.GooseDictationConfig({});
  return response.providers as Record<
    DictationProvider,
    DictationProviderStatus
  >;
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
  provider: DictationProvider,
  value: string,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationSecretSave({ provider, value });
}

export async function deleteDictationProviderSecret(
  provider: DictationProvider,
): Promise<void> {
  const client = await getClient();
  await client.goose.GooseDictationSecretDelete({ provider });
}

export async function listDictationLocalModels(): Promise<
  WhisperModelStatus[]
> {
  const client = await getClient();
  const response = await client.goose.GooseDictationModelsList({});
  return response.models;
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
  return response.progress ?? null;
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
