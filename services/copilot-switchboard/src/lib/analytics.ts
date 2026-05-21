import type { Env } from './types';

const KV_PREFIX = 'analytics';

export interface CopilotAnalytics {
  prs_reviewed: number;
  issues_handled: number;
  commits_pushed: number;
}

const EMPTY: CopilotAnalytics = {
  prs_reviewed: 0,
  issues_handled: 0,
  commits_pushed: 0,
};

export type AnalyticsEvent =
  | { kind: 'pr_reviewed' }
  | { kind: 'issue_handled' }
  | { kind: 'commit_pushed' };

function key(installationId: number): string {
  return `${KV_PREFIX}:${installationId}`;
}

export async function loadAnalytics(
  env: Env,
  installationId: number
): Promise<CopilotAnalytics> {
  const raw = await env.INSTALL_REGISTRY.get(key(installationId));
  if (!raw) return { ...EMPTY };
  try {
    const parsed = JSON.parse(raw) as Partial<CopilotAnalytics>;
    return { ...EMPTY, ...parsed };
  } catch {
    return { ...EMPTY };
  }
}

export async function recordEvent(
  env: Env,
  installationId: number,
  event: AnalyticsEvent
): Promise<void> {
  // KV has no atomic increments; for a single-user-per-install bot this
  // read-modify-write is fine. Multi-org rollouts will want a Durable Object.
  const current = await loadAnalytics(env, installationId);
  switch (event.kind) {
    case 'pr_reviewed':
      current.prs_reviewed += 1;
      break;
    case 'issue_handled':
      current.issues_handled += 1;
      break;
    case 'commit_pushed':
      current.commits_pushed += 1;
      break;
  }
  await env.INSTALL_REGISTRY.put(key(installationId), JSON.stringify(current));
}

export async function deleteAnalytics(env: Env, installationId: number): Promise<void> {
  await env.INSTALL_REGISTRY.delete(key(installationId));
}
