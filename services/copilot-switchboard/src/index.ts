/**
 * Trust model: the Worker holds zero user API keys and never sees user
 * code. Reviews run in the user's local goosed via the lapstone tunnel
 * using a per-install secret; KV stores opaque routing data only.
 *
 * Registration is authenticated by exchanging a GitHub OAuth code (issued
 * to the user during App install) and verifying via the GitHub API that
 * the user owns the claimed installation_id.
 */

import { verifyWebhookSignature } from './lib/github';
import {
  handleInstallation,
  handleIssueComment,
  handlePullRequest,
  handleRegister,
} from './lib/handlers';
import type {
  Env,
  InstallationEvent,
  IssueCommentEvent,
  PullRequestEvent,
  RegisterRequest,
} from './lib/types';

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    if (request.method === 'GET' && url.pathname === '/') {
      return new Response('Goose Copilot switchboard\n', {
        status: 200,
        headers: { 'content-type': 'text/plain' },
      });
    }

    if (request.method === 'POST' && url.pathname === '/webhook') {
      return handleWebhook(request, env, ctx);
    }

    if (request.method === 'POST' && url.pathname === '/copilot/register') {
      return handleRegisterRoute(request, env);
    }

    return new Response('Not Found', { status: 404 });
  },
};

async function handleWebhook(
  request: Request,
  env: Env,
  ctx: ExecutionContext
): Promise<Response> {
  const body = await request.text();
  const signature = request.headers.get('x-hub-signature-256') ?? '';

  if (!(await verifyWebhookSignature(body, signature, env.GITHUB_WEBHOOK_SECRET))) {
    return new Response('Unauthorized', { status: 401 });
  }

  const event = request.headers.get('x-github-event') ?? '';
  const deliveryId = request.headers.get('x-github-delivery') ?? '<no-delivery-id>';

  let payload: unknown;
  try {
    payload = JSON.parse(body);
  } catch {
    return new Response('Invalid JSON', { status: 400 });
  }

  // GitHub gives us a 5s budget to ack the webhook.
  switch (event) {
    case 'installation':
      ctx.waitUntil(guard(deliveryId, handleInstallation(payload as InstallationEvent, env)));
      break;
    case 'pull_request':
      ctx.waitUntil(guard(deliveryId, handlePullRequest(payload as PullRequestEvent, env)));
      break;
    case 'issue_comment':
      ctx.waitUntil(guard(deliveryId, handleIssueComment(payload as IssueCommentEvent, env)));
      break;
    case 'ping':
      break;
  }

  return new Response('OK', { status: 200 });
}

async function handleRegisterRoute(request: Request, env: Env): Promise<Response> {
  let body: RegisterRequest;
  try {
    body = (await request.json()) as RegisterRequest;
  } catch {
    return new Response('Invalid JSON', { status: 400 });
  }
  return handleRegister(body, env);
}

async function guard(deliveryId: string, p: Promise<void>): Promise<void> {
  try {
    await p;
  } catch (err) {
    console.error(`[handler] delivery=${deliveryId}: ${err instanceof Error ? err.stack : err}`);
  }
}
