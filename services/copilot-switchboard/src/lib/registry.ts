import type { Env, InstallRecord } from './types';

function installKey(installationId: number): string {
  return `install:${installationId}`;
}

function agentKey(agentId: string): string {
  return `agent:${agentId}`;
}

export async function loadInstall(
  env: Env,
  installationId: number
): Promise<InstallRecord | null> {
  const raw = await env.INSTALL_REGISTRY.get(installKey(installationId));
  if (!raw) return null;
  try {
    return JSON.parse(raw) as InstallRecord;
  } catch {
    return null;
  }
}

export async function loadInstallByAgent(
  env: Env,
  agentId: string
): Promise<InstallRecord | null> {
  const idRaw = await env.INSTALL_REGISTRY.get(agentKey(agentId));
  if (idRaw) {
    const id = Number.parseInt(idRaw, 10);
    if (Number.isFinite(id)) {
      const direct = await loadInstall(env, id);
      if (direct) return direct;
    }
  }

  // Pre-reverse-index installs: scan primary keys and backfill agent:<id>.
  const list = await env.INSTALL_REGISTRY.list({ prefix: 'install:', limit: 1000 });
  for (const item of list.keys) {
    const raw = await env.INSTALL_REGISTRY.get(item.name);
    if (!raw) continue;
    let record: InstallRecord;
    try {
      record = JSON.parse(raw) as InstallRecord;
    } catch {
      continue;
    }
    if (record.agentId !== agentId) continue;
    await env.INSTALL_REGISTRY.put(agentKey(agentId), String(record.installationId));
    return record;
  }
  return null;
}

export async function saveInstall(env: Env, record: InstallRecord): Promise<void> {
  await Promise.all([
    env.INSTALL_REGISTRY.put(installKey(record.installationId), JSON.stringify(record)),
    env.INSTALL_REGISTRY.put(agentKey(record.agentId), String(record.installationId)),
  ]);
}

export async function deleteInstall(env: Env, installationId: number): Promise<void> {
  const existing = await loadInstall(env, installationId);
  const tasks: Promise<void>[] = [env.INSTALL_REGISTRY.delete(installKey(installationId))];
  if (existing) {
    tasks.push(env.INSTALL_REGISTRY.delete(agentKey(existing.agentId)));
  }
  await Promise.all(tasks);
}
