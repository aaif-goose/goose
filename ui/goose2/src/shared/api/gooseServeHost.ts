import { invoke } from "@tauri-apps/api/core";

export interface GooseServeHostInfo {
  // Rename to baseUrl when goose serve supports a secure local origin.
  httpBaseUrl: string;
  secretKey: string;
}

export async function getGooseServeHostInfo(): Promise<GooseServeHostInfo> {
  return invoke<GooseServeHostInfo>("get_goose_serve_host_info");
}

export async function postGooseServeJson<TResponse>(
  path: string,
  body: unknown,
): Promise<TResponse> {
  const { httpBaseUrl, secretKey } = await getGooseServeHostInfo();
  const response = await fetch(`${httpBaseUrl}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Secret-Key": secretKey,
    },
    body: JSON.stringify(body),
  });

  if (!response.ok) {
    throw new Error(`Goose serve request failed (${response.status})`);
  }

  return (await response.json()) as TResponse;
}
