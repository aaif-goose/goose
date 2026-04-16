import { invoke } from "@tauri-apps/api/core";
import type { ProviderFieldValue } from "@/shared/types/providers";

export interface ProviderStatus {
  providerId: string;
  isConfigured: boolean;
}

export async function getProviderConfig(
  providerId: string,
): Promise<ProviderFieldValue[]> {
  return invoke("get_provider_config", { providerId });
}

export async function saveProviderField(
  key: string,
  value: string,
): Promise<void> {
  return invoke("save_provider_field", { key, value });
}

export async function deleteProviderConfig(providerId: string): Promise<void> {
  return invoke("delete_provider_config", { providerId });
}

export async function checkAllProviderStatus(): Promise<ProviderStatus[]> {
  return invoke("check_all_provider_status");
}

export async function restartApp(): Promise<void> {
  return invoke("restart_app");
}
