// Routing prefs cache — authoritative copy lives in goosed (see crates/goose/src/copilot/prefs.rs).
// DEFAULT_ROUTING_PREFS must match fixtures/routing_prefs_default.json (checked by Rust + vitest).

import type { Env } from './types';

export const CURRENT_SCHEMA_VERSION = 1;

const KV_PREFIX = 'routing';
const KV_TTL_SECONDS = 365 * 24 * 60 * 60;

export type TriggerPreference = 'pr-open' | 'on-every-push' | 'manual-only';
export type TriggerPermission = 'anyone' | 'write-access' | 'specific-users';

export interface RoutingPrefs {
  schema_version: number;
  auto_review_on_pr_open: boolean;
  trigger_preference: TriggerPreference;
  trigger_permission: TriggerPermission;
  allow_act_on_issues: boolean;
  specific_users_allowlist: string[];
}

export const DEFAULT_ROUTING_PREFS: RoutingPrefs = {
  schema_version: CURRENT_SCHEMA_VERSION,
  auto_review_on_pr_open: true,
  trigger_preference: 'pr-open',
  trigger_permission: 'anyone',
  allow_act_on_issues: false,
  specific_users_allowlist: [],
};

function key(installationId: number): string {
  return `${KV_PREFIX}:${installationId}`;
}

export async function loadRoutingPrefs(
  env: Env,
  installationId: number
): Promise<RoutingPrefs> {
  const raw = await env.INSTALL_REGISTRY.get(key(installationId));
  if (!raw) return DEFAULT_ROUTING_PREFS;
  try {
    const parsed = JSON.parse(raw) as RoutingPrefs;
    if (typeof parsed?.schema_version !== 'number') return DEFAULT_ROUTING_PREFS;
    if (parsed.schema_version > CURRENT_SCHEMA_VERSION) {
      return DEFAULT_ROUTING_PREFS;
    }
    // Merge so partial older payloads still produce a complete object.
    return { ...DEFAULT_ROUTING_PREFS, ...parsed };
  } catch {
    return DEFAULT_ROUTING_PREFS;
  }
}

export async function saveRoutingPrefs(
  env: Env,
  installationId: number,
  prefs: RoutingPrefs
): Promise<void> {
  await env.INSTALL_REGISTRY.put(key(installationId), JSON.stringify(prefs), {
    expirationTtl: KV_TTL_SECONDS,
  });
}

export async function deleteRoutingPrefs(
  env: Env,
  installationId: number
): Promise<void> {
  await env.INSTALL_REGISTRY.delete(key(installationId));
}

export function parseRoutingPrefs(body: unknown): RoutingPrefs {
  if (!body || typeof body !== 'object') {
    throw new Error('body must be a JSON object');
  }
  const b = body as Partial<RoutingPrefs>;

  const sv = b.schema_version;
  if (typeof sv !== 'number' || sv < 1) {
    throw new Error('schema_version must be a positive integer');
  }
  if (sv > CURRENT_SCHEMA_VERSION) {
    throw new Error(
      `schema_version ${sv} is newer than this switchboard understands (${CURRENT_SCHEMA_VERSION})`
    );
  }

  const triggerPref = b.trigger_preference;
  if (
    triggerPref !== 'pr-open' &&
    triggerPref !== 'on-every-push' &&
    triggerPref !== 'manual-only'
  ) {
    throw new Error('trigger_preference must be one of pr-open|on-every-push|manual-only');
  }

  const triggerPerm = b.trigger_permission;
  if (
    triggerPerm !== 'anyone' &&
    triggerPerm !== 'write-access' &&
    triggerPerm !== 'specific-users'
  ) {
    throw new Error(
      'trigger_permission must be one of anyone|write-access|specific-users'
    );
  }

  if (typeof b.auto_review_on_pr_open !== 'boolean') {
    throw new Error('auto_review_on_pr_open must be a boolean');
  }
  if (typeof b.allow_act_on_issues !== 'boolean') {
    throw new Error('allow_act_on_issues must be a boolean');
  }

  const rawAllowlist = b.specific_users_allowlist ?? [];
  if (!Array.isArray(rawAllowlist)) {
    throw new Error('specific_users_allowlist must be an array of strings');
  }
  const allowlist: string[] = [];
  for (const entry of rawAllowlist) {
    if (typeof entry !== 'string') {
      throw new Error('specific_users_allowlist entries must be strings');
    }
    const trimmed = entry.trim();
    if (trimmed.length > 0) allowlist.push(trimmed);
  }

  return {
    schema_version: sv,
    auto_review_on_pr_open: b.auto_review_on_pr_open,
    trigger_preference: triggerPref,
    trigger_permission: triggerPerm,
    allow_act_on_issues: b.allow_act_on_issues,
    specific_users_allowlist: allowlist,
  };
}
