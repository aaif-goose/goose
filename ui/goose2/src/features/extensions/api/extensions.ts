import { invoke } from "@tauri-apps/api/core";
import type { ExtensionConfig, ExtensionEntry } from "../types";

export function nameToKey(name: string): string {
  return name
    .replace(/\s/g, "")
    .replace(/[^a-zA-Z0-9_-]/g, "_")
    .toLowerCase();
}

export async function listExtensions(): Promise<ExtensionEntry[]> {
  return invoke("list_extensions");
}

export async function addExtension(
  name: string,
  extensionConfig: ExtensionConfig,
  enabled: boolean,
): Promise<void> {
  return invoke("add_extension", {
    name,
    extensionConfig,
    enabled,
  });
}

export async function removeExtension(configKey: string): Promise<void> {
  return invoke("remove_extension", { configKey });
}

export async function toggleExtension(
  configKey: string,
  enabled: boolean,
): Promise<void> {
  return invoke("toggle_extension", { configKey, enabled });
}
