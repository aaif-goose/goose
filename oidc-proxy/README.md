# OIDC Proxy

A Cloudflare Worker that authenticates GitHub Actions OIDC tokens and proxies requests to an upstream API with an injected API key. This lets CI workflows call APIs without storing long-lived secrets in GitHub.

## How it works

```
GitHub Actions (OIDC token) → Worker (validate JWT, inject API key) → Upstream API
```

1. A GitHub Actions workflow mints an OIDC token with a configured audience
2. The workflow sends requests to this proxy, passing the OIDC token as the API key
3. The worker validates the JWT against GitHub's JWKS, checks issuer/audience/age/repo
4. If valid, the request is forwarded to the upstream API with the real API key injected

## Setup

```bash
cd oidc-proxy
npm install
```

## Configuration

Edit `wrangler.toml` for your upstream:

| Variable | Description |
|---|---|
| `OIDC_ISSUER` | `https://token.actions.githubusercontent.com` |
| `OIDC_AUDIENCE` | The audience your workflow requests (e.g. `goose-oidc-proxy`) |
| `MAX_TOKEN_AGE_SECONDS` | Max age of OIDC token in seconds (default: `1200` = 20 min) |
| `MAX_REQUESTS_PER_TOKEN` | Max requests per OIDC token (default: `200`) |
| `RATE_LIMIT_PER_SECOND` | Max requests per second per token (default: `2`) |
| `ALLOWED_REPOS` | *(optional)* Comma-separated `owner/repo` list |
| `ALLOWED_REFS` | *(optional)* Comma-separated allowed refs |
| `UPSTREAM_URL` | The upstream API base URL |
| `UPSTREAM_AUTH_HEADER` | Header name for the API key (e.g. `x-api-key`, `Authorization`) |
| `UPSTREAM_AUTH_PREFIX` | *(optional)* Prefix before the key (e.g. `Bearer `) — omit for raw value |
| `CORS_ORIGIN` | *(optional)* Allowed CORS origin |
| `CORS_EXTRA_HEADERS` | *(optional)* Additional CORS allowed headers |

Set your upstream API key as a secret:

```bash
npx wrangler secret put UPSTREAM_API_KEY
```

### Example: Anthropic

```toml
UPSTREAM_URL = "https://api.anthropic.com"
UPSTREAM_AUTH_HEADER = "x-api-key"
CORS_EXTRA_HEADERS = "anthropic-version"
```

### Example: OpenAI-compatible

```toml
UPSTREAM_URL = "https://api.openai.com"
UPSTREAM_AUTH_HEADER = "Authorization"
UPSTREAM_AUTH_PREFIX = "Bearer "
```

## Usage in GitHub Actions

```yaml
permissions:
  id-token: write

steps:
  - name: Get OIDC token
    id: oidc
    uses: actions/github-script@v7
    with:
      script: |
        const token = await core.getIDToken('goose-oidc-proxy');
        core.setOutput('token', token);
        core.setSecret(token);

  - name: Call API through proxy
    env:
      ANTHROPIC_BASE_URL: https://oidc-proxy.your-subdomain.workers.dev
      ANTHROPIC_API_KEY: ${{ steps.oidc.outputs.token }}
    run: goose run --recipe my-recipe.yaml
```

## Testing

```bash
npm test
```

## Deploy

```bash
npx wrangler secret put UPSTREAM_API_KEY
npm run deploy
```

## Token budget and rate limiting

Each OIDC token is tracked by its `jti` (JWT ID) claim using a Durable Object. This provides:

- **Budget**: Each token is limited to `MAX_REQUESTS_PER_TOKEN` total requests (default: 200). Once exhausted, the proxy returns `429` with `{"error": "Token budget exhausted"}`.
- **Rate limit**: Each token is limited to `RATE_LIMIT_PER_SECOND` requests per second (default: 2). When exceeded, the proxy returns `429` with `{"error": "Rate limit exceeded"}` and a `Retry-After: 1` header.

Both limits are enforced atomically — the Durable Object processes one request at a time per token, so there are no race conditions.

## Token age vs expiry

GitHub OIDC tokens expire after ~5 minutes. For longer-running jobs, set `MAX_TOKEN_AGE_SECONDS` to allow recently-expired tokens. When set, the proxy checks the token's `iat` (issued-at) claim instead of `exp`.
