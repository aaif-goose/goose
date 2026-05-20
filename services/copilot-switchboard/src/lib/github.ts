const API = 'https://api.github.com';
const UA = 'goose-copilot-switchboard/0.1';

export async function verifyWebhookSignature(
  body: string,
  signatureHeader: string,
  secret: string
): Promise<boolean> {
  if (!signatureHeader.startsWith('sha256=')) return false;
  const provided = signatureHeader.slice('sha256='.length);

  const key = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign']
  );
  const sigBuf = await crypto.subtle.sign('HMAC', key, new TextEncoder().encode(body));
  const expected = bufToHex(sigBuf);

  return timingSafeEqual(provided, expected);
}

function bufToHex(buf: ArrayBuffer): string {
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let mismatch = 0;
  for (let i = 0; i < a.length; i++) mismatch |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return mismatch === 0;
}

export async function generateAppJwt(appId: string, privateKeyPem: string): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  const header = b64url(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  // iat is 60s in the past to tolerate clock skew; exp must be ≤ 10 min per GitHub.
  const payload = b64url(JSON.stringify({ iat: now - 60, exp: now + 540, iss: parseInt(appId, 10) }));
  const signingInput = `${header}.${payload}`;

  const key = await crypto.subtle.importKey(
    'pkcs8',
    pemToPkcs8(privateKeyPem),
    { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' },
    false,
    ['sign']
  );
  const sig = await crypto.subtle.sign(
    { name: 'RSASSA-PKCS1-v1_5' },
    key,
    new TextEncoder().encode(signingInput)
  );
  return `${signingInput}.${b64urlBytes(new Uint8Array(sig))}`;
}

function b64url(s: string): string {
  return b64urlBytes(new TextEncoder().encode(s));
}

function b64urlBytes(bytes: Uint8Array): string {
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function pemToPkcs8(pem: string): ArrayBuffer {
  const stripped = pem
    .replace(/-----BEGIN [A-Z ]+-----/, '')
    .replace(/-----END [A-Z ]+-----/, '')
    .replace(/\s+/g, '');
  const bin = atob(stripped);
  const buf = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
  return buf.buffer;
}

const installationTokenCache = new Map<number, { token: string; expiresAt: number }>();

export async function getInstallationToken(
  installationId: number,
  appJwt: string
): Promise<string> {
  const cached = installationTokenCache.get(installationId);
  if (cached && cached.expiresAt > Date.now() + 60_000) return cached.token;

  const res = await fetch(`${API}/app/installations/${installationId}/access_tokens`, {
    method: 'POST',
    headers: ghHeaders(appJwt),
  });
  if (!res.ok) throw await ghError(res, 'Failed to mint installation token');
  const data = (await res.json()) as { token: string; expires_at: string };
  installationTokenCache.set(installationId, {
    token: data.token,
    expiresAt: new Date(data.expires_at).getTime(),
  });
  return data.token;
}

interface InstallationReposPage {
  total_count: number;
  repositories: Array<{
    id: number;
    name: string;
    full_name: string;
    owner: { login: string } | null;
    visibility?: string;
    archived?: boolean;
    default_branch?: string;
    html_url?: string;
  }>;
}

const MAX_REPO_PAGES = 5;
const REPOS_PER_PAGE = 100;

export interface RepoSummary {
  id: number;
  full_name: string;
  name: string;
  owner: string;
  visibility: 'public' | 'private' | 'internal' | 'unknown';
  archived: boolean;
  default_branch: string;
  html_url: string;
}

export interface ListReposResult {
  total_count: number;
  repos: RepoSummary[];
  truncated: boolean;
}

export async function listInstallationRepos(token: string): Promise<ListReposResult> {
  const collected: RepoSummary[] = [];
  let totalCount = 0;
  let truncated = false;

  for (let page = 1; page <= MAX_REPO_PAGES; page++) {
    const res = await fetch(
      `${API}/installation/repositories?per_page=${REPOS_PER_PAGE}&page=${page}`,
      { headers: ghHeaders(token) }
    );
    if (!res.ok) throw await ghError(res, 'Failed to list installation repositories');
    const data = (await res.json()) as InstallationReposPage;
    totalCount = data.total_count ?? collected.length;
    for (const r of data.repositories) {
      collected.push({
        id: r.id,
        full_name: r.full_name,
        name: r.name,
        owner: r.owner?.login ?? '',
        visibility: normalizeVisibility(r.visibility),
        archived: r.archived ?? false,
        default_branch: r.default_branch ?? '',
        html_url: r.html_url ?? '',
      });
    }
    if (data.repositories.length < REPOS_PER_PAGE) break;
    if (page === MAX_REPO_PAGES && collected.length < totalCount) {
      truncated = true;
    }
  }

  return { total_count: totalCount, repos: collected, truncated };
}

function normalizeVisibility(v: string | undefined): RepoSummary['visibility'] {
  if (v === 'public' || v === 'private' || v === 'internal') return v;
  return 'unknown';
}

export async function getCommenterPermission(
  fullName: string,
  username: string,
  token: string
): Promise<'admin' | 'maintain' | 'write' | 'triage' | 'read' | 'none' | null> {
  const res = await fetch(
    `${API}/repos/${fullName}/collaborators/${encodeURIComponent(username)}/permission`,
    { headers: ghHeaders(token) }
  );
  if (!res.ok) return null;
  const data = (await res.json()) as { permission?: string };
  const p = data.permission;
  if (
    p === 'admin' ||
    p === 'maintain' ||
    p === 'write' ||
    p === 'triage' ||
    p === 'read' ||
    p === 'none'
  ) {
    return p;
  }
  return null;
}

export async function getBranchHeadSha(
  fullName: string,
  branch: string,
  token: string
): Promise<string> {
  const res = await fetch(
    `${API}/repos/${fullName}/git/refs/heads/${encodeURIComponent(branch)}`,
    { headers: ghHeaders(token) }
  );
  if (!res.ok) throw await ghError(res, `Failed to resolve branch ${branch}`);
  const data = (await res.json()) as { object?: { sha?: string } };
  const sha = data.object?.sha;
  if (!sha) throw new Error(`branch ${branch} resolved with no SHA`);
  return sha;
}

export async function getPullRequestHead(
  fullName: string,
  prNumber: number,
  token: string
): Promise<{ sha: string; ref: string; htmlUrl: string }> {
  const res = await fetch(`${API}/repos/${fullName}/pulls/${prNumber}`, {
    headers: ghHeaders(token),
  });
  if (!res.ok) throw await ghError(res, `Failed to load PR #${prNumber}`);
  const data = (await res.json()) as {
    head: { sha: string; ref: string };
    html_url: string;
  };
  return { sha: data.head.sha, ref: data.head.ref, htmlUrl: data.html_url };
}

export async function createCheckRun(
  fullName: string,
  headSha: string,
  token: string
): Promise<number> {
  const res = await fetch(`${API}/repos/${fullName}/check-runs`, {
    method: 'POST',
    headers: { ...ghHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: 'Goose Copilot',
      head_sha: headSha,
      status: 'in_progress',
      output: {
        title: 'Goose is reviewing this PR…',
        summary: 'Routing this review to your local goosed. Usually takes 30–90 seconds.',
      },
    }),
  });
  if (!res.ok) throw await ghError(res, 'Failed to create check run');
  const data = (await res.json()) as { id: number };
  return data.id;
}

export async function completeCheckRun(opts: {
  fullName: string;
  checkRunId: number;
  conclusion: 'success' | 'failure' | 'neutral' | 'cancelled';
  title: string;
  summary: string;
  token: string;
}): Promise<void> {
  const { fullName, checkRunId, conclusion, title, summary, token } = opts;
  const res = await fetch(`${API}/repos/${fullName}/check-runs/${checkRunId}`, {
    method: 'PATCH',
    headers: { ...ghHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({
      status: 'completed',
      conclusion,
      output: { title, summary: summary.slice(0, 65_000) },
    }),
  });
  if (!res.ok) throw await ghError(res, 'Failed to complete check run');
}

export async function postIssueComment(
  fullName: string,
  issueNumber: number,
  body: string,
  token: string
): Promise<void> {
  const res = await fetch(`${API}/repos/${fullName}/issues/${issueNumber}/comments`, {
    method: 'POST',
    headers: { ...ghHeaders(token), 'Content-Type': 'application/json' },
    body: JSON.stringify({ body }),
  });
  if (!res.ok) throw await ghError(res, 'Failed to post issue comment');
}

export type Reaction = '+1' | '-1' | 'laugh' | 'confused' | 'heart' | 'hooray' | 'rocket' | 'eyes';

export async function postCommentReaction(
  fullName: string,
  commentId: number,
  content: Reaction,
  token: string
): Promise<void> {
  const res = await fetch(
    `${API}/repos/${fullName}/issues/comments/${commentId}/reactions`,
    {
      method: 'POST',
      headers: { ...ghHeaders(token), 'Content-Type': 'application/json' },
      body: JSON.stringify({ content }),
    }
  );
  // 200 = already reacted, 201 = newly reacted; both are fine.
  if (!res.ok) throw await ghError(res, `Failed to react ${content}`);
}

function ghHeaders(token: string): HeadersInit {
  return {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
    'User-Agent': UA,
  };
}

async function ghError(res: Response, prefix: string): Promise<Error> {
  const detail = (await res.json().catch(() => ({}))) as { message?: string };
  return new Error(`${prefix}: ${res.status} ${detail.message ?? res.statusText}`);
}
