// Webhook handlers run inside `ctx.waitUntil()`; we've already returned 200
// to GitHub by the time they execute. Failures are logged, not rethrown.

import {
  completeCheckRun,
  createCheckRun,
  generateAppJwt,
  getCommenterPermission,
  getInstallationToken,
  getPullRequestHead,
  listInstallationRepos,
  postCommentReaction,
  postIssueComment,
} from './github';
import { exchangeCodeAndResolve } from './oauth';
import {
  deleteRoutingPrefs,
  loadRoutingPrefs,
  parseRoutingPrefs,
  saveRoutingPrefs,
  type TriggerPermission,
} from './prefs';
import { deleteInstall, loadInstall, loadInstallByAgent, saveInstall } from './registry';
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

/**
 * Persist the routing-prefs subset goosed pushed to us. Authenticated by the
 * install secret minted at register time — same secret the tunnel uses, so
 * no new credential surface.
 */
export async function handleRoutingPrefs(request: Request, env: Env): Promise<Response> {
  const installIdRaw = request.headers.get('x-install-id');
  const installSecret = request.headers.get('x-install-secret');
  if (!installIdRaw || !installSecret) {
    return jsonError(401, 'missing install credentials');
  }
  const installId = Number.parseInt(installIdRaw, 10);
  if (!Number.isFinite(installId)) {
    return jsonError(400, 'invalid install id');
  }

  const install = await loadInstall(env, installId);
  if (!install || install.tunnelSecret !== installSecret) {
    return jsonError(401, 'install credentials rejected');
  }

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return jsonError(400, 'invalid JSON body');
  }

  let prefs;
  try {
    prefs = parseRoutingPrefs(body);
  } catch (e) {
    return jsonError(400, e instanceof Error ? e.message : 'invalid routing prefs');
  }

  await saveRoutingPrefs(env, installId, prefs);
  return new Response(JSON.stringify({ ok: true, prefs }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

/**
 * `whoami`: given the (agent_id, tunnel_secret) any registered goosed has,
 * return its installation_id. This is the authoritative way for goosed to
 * learn its own install id when local config is empty (fresh install,
 * pre-this-code install, or wiped config).
 *
 * Auth is the same shape as the tunnel itself: agent_id + matching
 * tunnel_secret. Constant-time on success/failure paths.
 */
export async function handleWhoami(request: Request, env: Env): Promise<Response> {
  let body: { agent_id?: unknown; tunnel_secret?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return jsonError(400, 'invalid JSON body');
  }
  const agentId = typeof body.agent_id === 'string' ? body.agent_id : '';
  const tunnelSecret = typeof body.tunnel_secret === 'string' ? body.tunnel_secret : '';
  if (!agentId || !tunnelSecret) {
    return jsonError(400, 'agent_id and tunnel_secret are required');
  }

  const install = await loadInstallByAgent(env, agentId);
  if (!install || install.tunnelSecret !== tunnelSecret) {
    // Don't differentiate "no such agent" from "wrong secret" — same surface
    // either way, no oracle for attackers.
    return jsonError(401, 'unrecognized credentials');
  }

  return new Response(
    JSON.stringify({ installation_id: install.installationId }),
    { status: 200, headers: { 'content-type': 'application/json' } }
  );
}

/**
 * List the repositories accessible to the installation. Mints an install
 * token (cached) from the App JWT and calls GitHub's
 * `/installation/repositories` endpoint, paginated up to a hard cap.
 */
export async function handleListRepos(request: Request, env: Env): Promise<Response> {
  const installIdRaw = request.headers.get('x-install-id');
  const installSecret = request.headers.get('x-install-secret');
  if (!installIdRaw || !installSecret) {
    return jsonError(401, 'missing install credentials');
  }
  const installId = Number.parseInt(installIdRaw, 10);
  if (!Number.isFinite(installId)) {
    return jsonError(400, 'invalid install id');
  }

  const install = await loadInstall(env, installId);
  if (!install || install.tunnelSecret !== installSecret) {
    return jsonError(401, 'install credentials rejected');
  }

  try {
    const jwt = await generateAppJwt(env.GITHUB_APP_ID, env.GITHUB_APP_PRIVATE_KEY);
    const token = await getInstallationToken(installId, jwt);
    const result = await listInstallationRepos(token);
    return new Response(JSON.stringify(result), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`[repos] install=${installId}: ${msg}`);
    return jsonError(502, msg);
  }
}

// `created`/`added`/suspend/unsuspend are no-ops here — Desktop owns
// /register once the user enables Copilot. We only need to clean up KV on uninstall.
export async function handleInstallation(payload: InstallationEvent, env: Env): Promise<void> {
  if (payload.action === 'deleted') {
    await deleteInstall(env, payload.installation.id);
    await deleteRoutingPrefs(env, payload.installation.id);
  }
}

export async function handlePullRequest(payload: PullRequestEvent, env: Env): Promise<void> {
  if (!['opened', 'synchronize', 'reopened'].includes(payload.action)) return;
  if (payload.pull_request.draft) return;

  // Routing prefs gate.
  const routing = await loadRoutingPrefs(env, payload.installation.id);
  if (!routing.auto_review_on_pr_open) return;
  if (routing.trigger_preference === 'manual-only') return;
  if (payload.action === 'synchronize' && routing.trigger_preference === 'pr-open') return;

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

  // Trigger-permission gate. We check BEFORE reacting, so an unauthorized
  // commenter sees nothing (silent drop, no :eyes:).
  const routing = await loadRoutingPrefs(env, payload.installation.id);
  if (!(await commenterMayTrigger(routing.trigger_permission, payload, token))) {
    console.log(
      `[comment] ${payload.repository.full_name} #${payload.issue.number}: ` +
        `commenter ${payload.comment.user.login} blocked by trigger_permission=${routing.trigger_permission}`
    );
    return;
  }

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

/** Returns true when the commenter passes the configured trigger permission.
 *  Errors fail closed except for `'anyone'`, where we don't need a check at all. */
async function commenterMayTrigger(
  permission: TriggerPermission,
  payload: IssueCommentEvent,
  token: string
): Promise<boolean> {
  if (permission === 'anyone') return true;
  if (permission === 'specific-users') {
    // Allowlist storage not yet wired; treat as a deny-all to avoid surprises.
    return false;
  }
  // write-access: collaborator-permission lookup.
  const level = await getCommenterPermission(
    payload.repository.full_name,
    payload.comment.user.login,
    token
  );
  return level === 'admin' || level === 'maintain' || level === 'write';
}
