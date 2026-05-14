import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProviderConfigChangeResponse } from "@aaif/goose-sdk";
import { authenticateProviderConfig } from "./credentials";

interface ModelSetupOutput {
  providerId: string;
  line: string;
}

export async function authenticateModelProvider(
  providerId: string,
  providerLabel: string,
): Promise<ProviderConfigChangeResponse> {
  void providerLabel;
  return authenticateProviderConfig(providerId);
}

export function onModelSetupOutput(
  providerId: string,
  callback: (line: string) => void,
): Promise<UnlistenFn> {
  return listen<ModelSetupOutput>("model-setup:output", (event) => {
    if (event.payload.providerId === providerId) {
      callback(event.payload.line);
    }
  });
}
