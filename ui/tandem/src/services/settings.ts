import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  memoryDir?: string | null;
  projectsDir?: string | null;
}

export async function getSettings(): Promise<Settings> {
  const result = await invoke<Settings>("get_settings");
  return result ?? {};
}

export async function updateSettings(settings: Settings): Promise<Settings> {
  return await invoke<Settings>("update_settings", { settings });
}
