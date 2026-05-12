// Exchanges a GitHub App OAuth code for a user access token and verifies
// the user actually has access to the claimed installation_id.

const UA = 'goose-copilot-switchboard/0.1';

interface OAuthTokenResponse {
  access_token?: string;
  error?: string;
  error_description?: string;
}

interface InstallationListResponse {
  total_count: number;
  installations: Array<{ id: number }>;
}

export async function exchangeCodeAndVerify(opts: {
  clientId: string;
  clientSecret: string;
  code: string;
  installationId: number;
}): Promise<{ ok: true } | { ok: false; status: number; error: string }> {
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
  const owned = installs.installations.some((i) => i.id === opts.installationId);
  if (!owned) {
    return {
      ok: false,
      status: 403,
      error: 'authenticated user does not own the requested installation',
    };
  }
  return { ok: true };
}
