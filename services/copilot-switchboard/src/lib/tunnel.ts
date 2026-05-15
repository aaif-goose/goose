// lapstone forwards any request hitting `/tunnel/<agent_id>/*` to
// `127.0.0.1:<goosed_port>/*` after validating `X-Secret-Key`. So we just
// POST `<tunnel_url>/copilot/<endpoint>` with the per-install secret.

import type { InstallRecord } from './types';

export interface TunnelRunParams {
  githubToken: string;
  repo: string;
  prNumber: number;
  headSha: string;
  prUrl: string;
  checkRunId?: number;
}

export interface TunnelCommentParams {
  githubToken: string;
  repo: string;
  prNumber: number;
  headSha: string;
  headRef: string;
  prUrl: string;
  commentBody: string;
  commenter: string;
}

export interface TunnelRunResult {
  ok: boolean;
  status: number;
  body: string;
}

const TUNNEL_TIMEOUT_MS = 20_000;

async function postJson(
  install: InstallRecord,
  path: string,
  body: unknown
): Promise<TunnelRunResult> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TUNNEL_TIMEOUT_MS);
  try {
    const res = await fetch(`${install.tunnelUrl}${path}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': install.tunnelSecret,
      },
      signal: controller.signal,
      body: JSON.stringify(body),
    });
    const text = await res.text();
    return { ok: res.ok, status: res.status, body: text };
  } finally {
    clearTimeout(timeout);
  }
}

export async function runReviewViaTunnel(
  install: InstallRecord,
  params: TunnelRunParams
): Promise<TunnelRunResult> {
  return postJson(install, '/copilot/review', {
    github_token: params.githubToken,
    repo: params.repo,
    pr_number: params.prNumber,
    head_sha: params.headSha,
    pr_url: params.prUrl,
    check_run_id: params.checkRunId,
  });
}

export async function runCommentViaTunnel(
  install: InstallRecord,
  params: TunnelCommentParams
): Promise<TunnelRunResult> {
  return postJson(install, '/copilot/comment', {
    github_token: params.githubToken,
    repo: params.repo,
    pr_number: params.prNumber,
    head_sha: params.headSha,
    head_ref: params.headRef,
    pr_url: params.prUrl,
    comment_body: params.commentBody,
    commenter: params.commenter,
  });
}
