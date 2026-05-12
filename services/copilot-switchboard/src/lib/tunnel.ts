// lapstone forwards any request hitting `/tunnel/<agent_id>/*` to
// `127.0.0.1:<goosed_port>/*` after validating `X-Secret-Key`. So we just
// POST `<tunnel_url>/copilot/review` with the per-install secret.

import type { InstallRecord } from './types';

export interface TunnelRunParams {
  githubToken: string;
  repo: string;
  prNumber: number;
  headSha: string;
  prUrl: string;
  checkRunId?: number;
}

export interface TunnelRunResult {
  ok: boolean;
  status: number;
  body: string;
}

const TUNNEL_TIMEOUT_MS = 20_000;

export async function runReviewViaTunnel(
  install: InstallRecord,
  params: TunnelRunParams
): Promise<TunnelRunResult> {
  const target = `${install.tunnelUrl}/copilot/review`;

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TUNNEL_TIMEOUT_MS);

  try {
    const res = await fetch(target, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': install.tunnelSecret,
      },
      signal: controller.signal,
      body: JSON.stringify({
        github_token: params.githubToken,
        repo: params.repo,
        pr_number: params.prNumber,
        head_sha: params.headSha,
        pr_url: params.prUrl,
        check_run_id: params.checkRunId,
      }),
    });
    const body = await res.text();
    return { ok: res.ok, status: res.status, body };
  } finally {
    clearTimeout(timeout);
  }
}
