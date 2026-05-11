import type {
  ProviderConfigChangeResponse,
  ProviderConfigFieldUpdate,
  ProviderConfigStatusDto,
} from "@aaif/goose-sdk";
import type { ProviderFieldValue } from "@/shared/types/providers";
import { getClient } from "@/shared/api/acpConnection";

export type ProviderStatus = ProviderConfigStatusDto;
export type ProviderFieldSaveInput = ProviderConfigFieldUpdate;

export async function getProviderConfig(
  providerId: string,
): Promise<ProviderFieldValue[]> {
  const client = await getClient();
  const response = await client.goose.GooseProvidersConfigRead({ providerId });
  return response.fields;
}

export async function saveProviderConfig(
  providerId: string,
  fields: ProviderFieldSaveInput[],
): Promise<ProviderConfigChangeResponse> {
  const client = await getClient();
  return client.goose.GooseProvidersConfigSave({ providerId, fields });
}

export async function authenticateProviderConfig(
  providerId: string,
): Promise<ProviderConfigChangeResponse> {
  const client = await getClient();
  return client.goose.GooseProvidersConfigAuthenticate({ providerId });
}

export async function deleteProviderConfig(
  providerId: string,
): Promise<ProviderConfigChangeResponse> {
  const client = await getClient();
  return client.goose.GooseProvidersConfigDelete({ providerId });
}

export async function checkAllProviderStatus(): Promise<ProviderStatus[]> {
  const client = await getClient();
  const response = await client.goose.GooseProvidersConfigStatus({
    providerIds: [],
  });
  return response.statuses;
}
