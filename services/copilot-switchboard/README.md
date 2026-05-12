# Goose Copilot — Switchboard

A small Cloudflare Worker that bridges GitHub webhooks → the user's local
`goosed` instance via the existing lapstone tunnel.

The switchboard is the **only** Block-hosted component of Goose Copilot.
It deliberately holds no API keys and never sees user code.

## Architecture

```
github.com  ──webhook──▶  [switchboard Worker]
                              │ verify HMAC
                              │ mint App installation token
                              │ look up installation_id → tunnel routing
                              ▼
                   [lapstone tunnel proxy] ──WS──▶  user's local goosed
                                                       │ goose run
                                                       │   --recipe goose-copilot-review
                                                       │   --params pr=...
                                                       ▼
                                                   inline review on PR
```

What the switchboard sees:
- Webhook payloads (HMAC-validated)
- Opaque routing data: `installation_id → {agent_id, tunnel_secret, tunnel_url}`

What the switchboard does **not** see:
- User model API keys (live in the user's keyring)
- User source code (the diff is fetched by the user's local goosed)
- Review content (generated locally, posted to GitHub from the user's machine)

## GitHub App configuration

Permissions:
- Contents: Read
- Pull requests: Read & write
- Issues: Read & write (for the `@goose-copilot review` mention)
- Checks: Read & write
- Metadata: Read

Webhook subscriptions:
- `installation`
- `pull_request`
- `issue_comment`

Webhook URL: `https://<your-worker>.workers.dev/webhook`
Webhook secret: same string set as `GITHUB_WEBHOOK_SECRET`.

## Local development

```bash
pnpm install
pnpm dev          # wrangler dev, hot reload
pnpm test         # vitest
pnpm typecheck    # tsc --noEmit
```

## Deploy

1. **Create the KV namespace.**
   ```bash
   wrangler kv namespace create INSTALL_REGISTRY
   ```
   Paste the returned id into `wrangler.toml` (`kv_namespaces[0].id`).

2. **Set secrets.**
   ```bash
   wrangler secret put GITHUB_APP_ID
   wrangler secret put GITHUB_APP_PRIVATE_KEY      # paste the full PEM
   wrangler secret put GITHUB_WEBHOOK_SECRET
   wrangler secret put REGISTER_SHARED_SECRET      # any high-entropy string
   ```

3. **Deploy.**
   ```bash
   pnpm deploy
   ```

4. **Set the webhook URL** in the GitHub App settings to
   `https://<your-worker>.workers.dev/webhook`.

5. **Wire Desktop** to the same `REGISTER_SHARED_SECRET` and Worker base URL
   so the Copilot tab can POST `/register`.

## Endpoints

### `POST /webhook`
HMAC-validated GitHub webhook. Returns 200 immediately and processes events
in the background.

### `POST /register`
Called by Goose Desktop when the user enables Copilot reviews.

```http
POST /register
Authorization: Bearer <REGISTER_SHARED_SECRET>
Content-Type: application/json

{
  "installationId": 12345678,
  "agentId":        "0123abcd...",
  "tunnelSecret":   "abcdef0123...",
  "tunnelUrl":      "https://tunnel-proxy.example/tunnel/0123abcd..."
}
```

Stores the routing record in KV. The Worker uses it on the next webhook
to dispatch the review to the user's tunnel.

### `DELETE /register`
Same auth, body `{ "installationId": 12345678 }`. Removes the KV record so
future PRs no longer attempt to reach the user's goosed.

## Transferring out of this monorepo

Everything in `services/copilot-switchboard/` is self-contained: zero
imports from `crates/` or `ui/`. Move the directory to its own repo and
nothing inside needs editing.
