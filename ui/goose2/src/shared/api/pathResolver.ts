import { invoke } from "@tauri-apps/api/core";

export interface ResolvePathParams {
  parts: string[];
}

export interface ResolvedPath {
  path?: string;
}

export async function resolvePath({
  parts,
}: ResolvePathParams): Promise<ResolvedPath> {
  return invoke("resolve_path", {
    request: { parts },
  });
}

export async function resolveOptionalPath(
  parts?: string[] | null,
): Promise<string | undefined> {
  if (!parts) {
    return undefined;
  }

  return (await resolvePath({ parts })).path;
}
