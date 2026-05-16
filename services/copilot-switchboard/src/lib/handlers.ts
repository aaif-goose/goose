// Webhook handlers run inside `ctx.waitUntil()`; we've already returned 200
// to GitHub by the time they execute. Failures are logged, not rethrown.

import {
  completeCheckRun,
  createCheckRun,
  generateAppJwt,
  getInstallationToken,
  getPullRequestHead,
  postCommentReaction,
  postIssueComment,
} from './github';
import { exchangeCodeAndResolve } from './oauth';
import { deleteInstall, loadInstall, saveInstall } from './registry';
import { runCommentViaTunnel, runReviewViaTunnel } from './tunnel';
import type {
  Env,
  InstallationEvent,
  IssueCommentEvent,
  PullRequestEvent,
  RegisterRequest,
} from './types';

const MENTION_RE = /(^|\s)@goose-copilot\b/i;
const REVIEW_TRIGGER_RE = /(^|\s)@goose-copilot\s+review\s*$/im;

export async function handleRegister(req: RegisterRequest, env: Env): Promise<Response> {
  if (
    !req ||
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

  const resolution = await exchangeCodeAndResolve({
    clientId: env.GITHUB_OAUTH_CLIENT_ID,
    clientSecret: env.GITHUB_OAUTH_CLIENT_SECRET,
    appId: Number(env.GITHUB_APP_ID),
    code: req.oauth_code,
  });
  if (!resolution.ok) {
    return jsonError(resolution.status, resolution.error);
  }

  await saveInstall(env, {
    installationId: resolution.installationId,
    agentId: req.agent_id,
    tunnelSecret: req.tunnel_secret,
    tunnelUrl: req.tunnel_url,
    registeredAt: new Date().toISOString(),
  });

  return new Response(
    JSON.stringify({
      ok: true,
      installation_id: resolution.installationId,
      account_login: resolution.accountLogin,
    }),
    {
      status: 200,
      headers: { 'content-type': 'application/json' },
    }
  );
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
  // Bot accounts trigger nothing — prevents goose-copilot[bot] from replying
  // to its own comments and other bots (Dependabot, CodeRabbit, etc).
  if (payload.comment.user.type === 'Bot') return;
  if (!MENTION_RE.test(payload.comment.body)) return;

  const jwt = await generateAppJwt(env.GITHUB_APP_ID, env.GITHUB_APP_PRIVATE_KEY);
  const token = await getInstallationToken(payload.installation.id, jwt);
  const pr = await getPullRequestHead(payload.repository.full_name, payload.issue.number, token);

  // Ack the mention right away so the user knows the webhook fired. goosed
  // will react with :+1: / :confused: once it's done.
  await postCommentReaction(
    payload.repository.full_name,
    payload.comment.id,
    'eyes',
    token
  ).catch((err) =>
    console.warn(
      `[comment] ${payload.repository.full_name} #${payload.issue.number}: :eyes: reaction failed: ${err}`
    )
  );

  if (REVIEW_TRIGGER_RE.test(payload.comment.body)) {
    await triggerReview({
      fullName: payload.repository.full_name,
      prNumber: payload.issue.number,
      headSha: pr.sha,
      prUrl: pr.htmlUrl,
      installationId: payload.installation.id,
      env,
      token,
      commentId: payload.comment.id,
    });
    return;
  }

  await triggerComment({
    fullName: payload.repository.full_name,
    prNumber: payload.issue.number,
    headSha: pr.sha,
    headRef: pr.ref,
    prUrl: pr.htmlUrl,
    commentBody: payload.comment.body,
    commenter: payload.comment.user.login,
    commentId: payload.comment.id,
    installationId: payload.installation.id,
    env,
    token,
  });
}

async function triggerComment(opts: {
  fullName: string;
  prNumber: number;
  headSha: string;
  headRef: string;
  prUrl: string;
  commentBody: string;
  commenter: string;
  commentId: number;
  installationId: number;
  env: Env;
  token: string;
}): Promise<void> {
  const { fullName, prNumber, commentBody, commenter, installationId, env, token } = opts;

  const install = await loadInstall(env, installationId);
  if (!install) {
    // No registered goosed — surface that as a polite reply rather than silence.
    await postIssueComment(
      fullName,
      prNumber,
      `@${commenter} Goose Copilot is installed on this repo but no local goose is connected. ` +
        `Open Goose Desktop → Copilot tab → Connect GitHub to enable.`,
      token
    ).catch((err) =>
      console.error(`[comment] ${fullName} #${prNumber}: post no-install reply failed: ${err}`)
    );
    return;
  }

  try {
    const result = await runCommentViaTunnel(install, {
      githubToken: token,
      repo: fullName,
      prNumber: opts.prNumber,
      headSha: opts.headSha,
      headRef: opts.headRef,
      prUrl: opts.prUrl,
      commentBody,
      commenter,
      commentId: opts.commentId,
    });
    if (!result.ok) {
      throw new Error(`tunnel responded ${result.status}: ${result.body.slice(0, 200)}`);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`[comment] ${fullName} #${prNumber}: ${msg}`);
    try {
      await postIssueComment(
        fullName,
        prNumber,
        `@${commenter} Goose Copilot couldn't reach your local goose. ` +
          `Make sure Goose Desktop is running, then try again.`,
        token
      );
    } catch (postErr) {
      console.error(
        `[comment] ${fullName} #${prNumber}: also failed to post error reply: ${
          postErr instanceof Error ? postErr.message : postErr
        }`
      );
    }
  }
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
  /** When the review was triggered via `@goose-copilot review`, goosed reacts on this comment when done. */
  commentId?: number;
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
      commentId: opts.commentId,
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
