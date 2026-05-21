const UA = 'goose-copilot-switchboard/0.1';

interface OAuthTokenResponse {
  access_token?: string;
  error?: string;
  error_description?: string;
}

interface InstallationListResponse {
  total_count: number;
  installations: Array<{ id: number; app_id: number; account: { login: string } | null }>;
}

export type ResolveResult =
  | { ok: true; installationId: number; accountLogin: string | null }
  | { ok: false; status: number; error: string };

export async function exchangeCodeAndResolve(opts: {
  clientId: string;
  clientSecret: string;
  appId: number;
  code: string;
}): Promise<ResolveResult> {
  const tokenRes = await fetch('https://github.com/login/oauth/access_token', {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
      'User-Agent': UA,
    },
    body: JSON.stringify({
      client_id: opts.clientId,
      client_secret: opts.clientSecret,
      code: opts.code,
    }),
  });

  if (!tokenRes.ok) {
    return { ok: false, status: 502, error: `code exchange failed: ${tokenRes.status}` };
  }
  const tokenBody = (await tokenRes.json()) as OAuthTokenResponse;
  if (tokenBody.error || !tokenBody.access_token) {
    return {
      ok: false,
      status: 400,
      error: `oauth error: ${tokenBody.error ?? 'no access_token returned'}`,
    };
  }

  const userToken = tokenBody.access_token;
  const installRes = await fetch('https://api.github.com/user/installations?per_page=100', {
    headers: {
      Authorization: `Bearer ${userToken}`,
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
      'User-Agent': UA,
    },
  });
  if (!installRes.ok) {
    return {
      ok: false,
      status: 502,
      error: `installations lookup failed: ${installRes.status}`,
    };
  }
  const installs = (await installRes.json()) as InstallationListResponse;
  const mine = installs.installations.filter((i) => i.app_id === opts.appId);
  if (mine.length === 0) {
    return {
      ok: false,
      status: 409,
      error:
        'Goose Copilot is not installed on any account this user can access. Install the GitHub App first.',
    };
  }
  // Pick the first matching installation. Most users have exactly one
  // (their personal account). Multi-org users will be revisited later.
  const chosen = mine[0];
  return {
    ok: true,
    installationId: chosen.id,
    accountLogin: chosen.account?.login ?? null,
  };
}
