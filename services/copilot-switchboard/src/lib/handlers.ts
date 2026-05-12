// Webhook handlers run inside `ctx.waitUntil()`; we've already returned 200
// to GitHub by the time they execute. Failures are logged, not rethrown.

import {
  completeCheckRun,
  createCheckRun,
  generateAppJwt,
  getInstallationToken,
  getPullRequestHead,
} from './github';
import { exchangeCodeAndVerify } from './oauth';
import { deleteInstall, loadInstall, saveInstall } from './registry';
import { runReviewViaTunnel } from './tunnel';
import type {
  Env,
  InstallationEvent,
  IssueCommentEvent,
  PullRequestEvent,
  RegisterRequest,
} from './types';

const REVIEW_TRIGGER_RE = /(^|\s)@goose-copilot\s+review\b/i;

export async function handleRegister(req: RegisterRequest, env: Env): Promise<Response> {
  if (
    !req ||
    typeof req.installation_id !== 'number' ||
    typeof req.oauth_code !== 'string' ||
    typeof req.agent_id !== 'string' ||
    typeof req.tunnel_secret !== 'string' ||
    typeof req.tunnel_url !== 'string'
  ) {
    return jsonError(400, 'Missing or invalid registration fields');
  }
  if (!/^https:\/\//.test(req.tunnel_url)) {
    return jsonError(400, 'tunnel_url must be an https URL');
  }

  const verification = await exchangeCodeAndVerify({
    clientId: env.GITHUB_OAUTH_CLIENT_ID,
    clientSecret: env.GITHUB_OAUTH_CLIENT_SECRET,
    code: req.oauth_code,
    installationId: req.installation_id,
  });
  if (!verification.ok) {
    return jsonError(verification.status, verification.error);
  }

  await saveInstall(env, {
    installationId: req.installation_id,
    agentId: req.agent_id,
    tunnelSecret: req.tunnel_secret,
    tunnelUrl: req.tunnel_url,
    registeredAt: new Date().toISOString(),
  });

  return new Response(JSON.stringify({ ok: true }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

function jsonError(status: number, message: string): Response {
  return new Response(JSON.stringify({ error: message }), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// `created`/`added`/suspend/unsuspend are no-ops here — Desktop owns
// /register once the user enables Copilot. We only need to clean up KV on uninstall.
export async function handleInstallation(payload: InstallationEvent, env: Env): Promise<void> {
  if (payload.action === 'deleted') {
    await deleteInstall(env, payload.installation.id);
  }
}

export async function handlePullRequest(payload: PullRequestEvent, env: Env): Promise<void> {
  if (!['opened', 'synchronize', 'reopened'].includes(payload.action)) return;
  if (payload.pull_request.draft) return;

  await triggerReview({
    fullName: payload.repository.full_name,
    prNumber: payload.number,
    headSha: payload.pull_request.head.sha,
    prUrl: payload.pull_request.html_url,
    installationId: payload.installation.id,
    env,
  });
}

export async function handleIssueComment(payload: IssueCommentEvent, env: Env): Promise<void> {
  if (payload.action !== 'created') return;
  if (!payload.issue.pull_request) return;
  if (!REVIEW_TRIGGER_RE.test(payload.comment.body)) return;

  const jwt = await generateAppJwt(env.GITHUB_APP_ID, env.GITHUB_APP_PRIVATE_KEY);
  const token = await getInstallationToken(payload.installation.id, jwt);
  const pr = await getPullRequestHead(payload.repository.full_name, payload.issue.number, token);

  await triggerReview({
    fullName: payload.repository.full_name,
    prNumber: payload.issue.number,
    headSha: pr.sha,
    prUrl: pr.htmlUrl,
    installationId: payload.installation.id,
    env,
    token,
  });
}

async function triggerReview(opts: {
  fullName: string;
  prNumber: number;
  headSha: string;
  prUrl: string;
  installationId: number;
  env: Env;
  /** Reuse a freshly-minted token from the caller if we already have one. */
  token?: string;
}): Promise<void> {
  const { fullName, prNumber, headSha, prUrl, installationId, env } = opts;

  const install = await loadInstall(env, installationId);
  if (!install) {
    await postNeutralCheck({
      fullName,
      headSha,
      installationId,
      env,
      title: 'Goose Copilot not enabled',
      summary:
        'No local goosed is registered for this installation. Open Goose Desktop → Copilot tab → enable code review to start receiving reviews.',
      token: opts.token,
    }).catch(() => {});
    return;
  }

  const token =
    opts.token ??
    (await getInstallationToken(
      installationId,
      await generateAppJwt(env.GITHUB_APP_ID, env.GITHUB_APP_PRIVATE_KEY)
    ));

  let checkRunId: number | undefined;
  try {
    checkRunId = await createCheckRun(fullName, headSha, token);
  } catch (err) {
    console.warn(
      `[trigger] ${fullName} #${prNumber}: check run creation failed (continuing): ${
        err instanceof Error ? err.message : err
      }`
    );
  }

  try {
    const result = await runReviewViaTunnel(install, {
      githubToken: token,
      repo: fullName,
      prNumber,
      headSha,
      prUrl,
      checkRunId,
    });

    if (!result.ok) {
      throw new Error(`tunnel responded ${result.status}: ${result.body.slice(0, 200)}`);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`[trigger] ${fullName} #${prNumber}: ${msg}`);
    if (checkRunId !== undefined) {
      try {
        await completeCheckRun({
          fullName,
          checkRunId,
          conclusion: 'neutral',
          title: 'Goose Copilot offline',
          summary: `Could not reach your local goosed via the tunnel:\n\n${msg}\n\nMake sure Goose Desktop is running and try \`@goose-copilot review\` to retry.`,
          token,
        });
      } catch (completeErr) {
        console.error(
          `[trigger] ${fullName} #${prNumber}: also failed to complete check run: ${
            completeErr instanceof Error ? completeErr.message : completeErr
          }`
        );
      }
    }
  }
}

async function postNeutralCheck(opts: {
  fullName: string;
  headSha: string;
  installationId: number;
  env: Env;
  title: string;
  summary: string;
  token?: string;
}): Promise<void> {
  const token =
    opts.token ??
    (await getInstallationToken(
      opts.installationId,
      await generateAppJwt(opts.env.GITHUB_APP_ID, opts.env.GITHUB_APP_PRIVATE_KEY)
    ));
  const checkRunId = await createCheckRun(opts.fullName, opts.headSha, token);
  await completeCheckRun({
    fullName: opts.fullName,
    checkRunId,
    conclusion: 'neutral',
    title: opts.title,
    summary: opts.summary,
    token,
  });
}
