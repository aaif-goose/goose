import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface ModelSetupOutput {
  providerId: string;
  line: string;
}

export async function authenticateModelProvider(
  providerId: string,
  providerLabel: string,
): Promise<void> {
  return invoke("authenticate_model_provider", { providerId, providerLabel });
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
