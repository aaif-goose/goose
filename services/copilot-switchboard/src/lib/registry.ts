import type { Env, InstallRecord } from './types';

/**
 * KV layout:
 *   install:<installation_id>   → InstallRecord
 *   agent:<agent_id>            → installation_id (number, JSON-encoded)
 *
 * The reverse index lets us answer "what installation does this tunnel
 * belong to?" in a single KV read instead of scanning every install
 * record. Written and deleted atomically with the primary record.
 */

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

  // Reverse-index miss: either (a) the install pre-dates the reverse index
  // or (b) the secondary write failed during saveInstall. Fall back to a
  // bounded linear scan over the primary records, and opportunistically
  // backfill the reverse index for next time.
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
    // Found it. Backfill the reverse index so the next call is O(1).
    await env.INSTALL_REGISTRY.put(agentKey(agentId), String(record.installationId));
    return record;
  }
  return null;
}

export async function saveInstall(env: Env, record: InstallRecord): Promise<void> {
  // Best-effort atomicity: write both keys; if the second fails the primary
  // still wins and a future `whoami` linear-scan fallback could recover.
  // CF KV has no transactions, so we accept the small inconsistency window.
  await Promise.all([
    env.INSTALL_REGISTRY.put(installKey(record.installationId), JSON.stringify(record)),
    env.INSTALL_REGISTRY.put(agentKey(record.agentId), String(record.installationId)),
  ]);
}

export async function deleteInstall(env: Env, installationId: number): Promise<void> {
  // Read the record first to learn the agentId so we can drop the reverse
  // index too. Tolerates a missing primary (already deleted).
  const existing = await loadInstall(env, installationId);
  const tasks: Promise<void>[] = [env.INSTALL_REGISTRY.delete(installKey(installationId))];
  if (existing) {
    tasks.push(env.INSTALL_REGISTRY.delete(agentKey(existing.agentId)));
  }
  await Promise.all(tasks);
}
