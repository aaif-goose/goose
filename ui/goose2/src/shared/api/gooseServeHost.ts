import { invoke } from "@tauri-apps/api/core";

export interface GooseServeHostInfo {
  // Rename to baseUrl when goose serve supports a secure local origin.
  httpBaseUrl: string;
  secretKey: string;
}

export async function getGooseServeHostInfo(): Promise<GooseServeHostInfo> {
  return invoke<GooseServeHostInfo>("get_goose_serve_host_info");
}
