import type { Env, InstallRecord } from './types';

function key(installationId: number): string {
  return `install:${installationId}`;
}

export async function loadInstall(
  env: Env,
  installationId: number
): Promise<InstallRecord | null> {
  const raw = await env.INSTALL_REGISTRY.get(key(installationId));
  if (!raw) return null;
  try {
    return JSON.parse(raw) as InstallRecord;
  } catch {
    return null;
  }
}

export async function saveInstall(env: Env, record: InstallRecord): Promise<void> {
  await env.INSTALL_REGISTRY.put(key(record.installationId), JSON.stringify(record));
}

export async function deleteInstall(env: Env, installationId: number): Promise<void> {
  await env.INSTALL_REGISTRY.delete(key(installationId));
}
