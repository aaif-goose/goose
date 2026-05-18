---
sidebar_position: 90
title: Running a Remote goose Server
sidebar_label: Remote Server
---

# Running a Remote goose Server

goose Desktop normally runs its own `goosed` server process in the background on the same machine. You can also run `goosed` separately — for example, on a remote VM or a different machine on your network — and point goose Desktop at it.

This is useful when you want goose to run somewhere with more compute, a stable IP, or shared access, while still driving it from a local Desktop UI.

This guide covers:

1. [Starting a `goosed` server on a remote machine](#1-start-the-goosed-server)
2. [Verifying it is reachable](#2-verify-the-server-is-up)
3. [Locating the certificate fingerprint](#3-find-the-certificate-fingerprint)
4. [Configuring goose Desktop to connect to it](#4-configure-goose-desktop)
5. [Running `goosed` as a background service on macOS](#running-goosed-as-a-background-service-macos)
6. [Troubleshooting](#troubleshooting)

:::warning TLS is required
goose Desktop will refuse to connect to a remote `goosed` server over plain HTTP. TLS is enabled by default (`GOOSE_TLS=true`), so make sure you have not disabled it.
:::

## Initial Setup

### 1. Start the `goosed` server

On the remote machine, launch `goosed` with the host, port, TLS, and a shared secret key:

```bash
GOOSE_HOST=0.0.0.0 \
GOOSE_PORT=3000 \
GOOSE_TLS=true \
GOOSE_SERVER__SECRET_KEY='YOUR_SECRET' \
/Applications/Goose.app/Contents/Resources/bin/goosed agent
```

On Linux or Windows the path to the `goosed` binary will differ — use the one bundled with your goose installation, or a standalone `goosed` build.

| Variable | Purpose |
|----------|---------|
| `GOOSE_HOST` | Interface to bind to. Use `0.0.0.0` to accept connections from other machines. Binding to `localhost` or `127.0.0.1` will only accept local connections. |
| `GOOSE_PORT` | TCP port to listen on. |
| `GOOSE_TLS` | Must be `true`. goose Desktop will not connect to a plain HTTP server. |
| `GOOSE_SERVER__SECRET_KEY` | Shared secret. The client must send this in the `X-Secret-Key` header. Treat it like a password. |

:::tip
Pick a long, random value for `GOOSE_SERVER__SECRET_KEY` and store it in a password manager — the same value goes into goose Desktop later.
:::

### 2. Verify the server is up

First, confirm `goosed` is actually listening on the port you expect:

```bash
lsof -nP -iTCP:3000 -sTCP:LISTEN
```

Then test the endpoints from the server itself. The `-k` flag tells `curl` to accept the self-signed TLS certificate that `goosed` generates:

```bash
# Connectivity only
curl -i https://127.0.0.1:3000/status -k

# Authenticated endpoint (real test)
curl -i https://127.0.0.1:3000/config/read -k \
  -H 'Content-Type: application/json' \
  -H 'X-Secret-Key: YOUR_SECRET' \
  --data '{"key":"GOOSE_PROVIDER","is_secret":false}'
```

A `200` response from the second call confirms that TLS is up, the secret key is being accepted, and the server is ready to receive client requests.

If you intend to reach the server from another machine, also test from there using the server's hostname or VPN address — not `127.0.0.1`.

### 3. Find the certificate fingerprint

Because `goosed` generates a self-signed TLS certificate, goose Desktop pins it by SHA-256 fingerprint rather than relying on a public certificate authority.

When TLS is enabled, `goosed` logs the fingerprint on startup. It looks like:

```text
GOOSED_CERT_FINGERPRINT=AA:BB:CC:DD:EE:FF:...
```

To capture it, either:

- Run `goosed` interactively and read it from the terminal output, or
- Tail the log file you redirect to when running as a service (see [Running `goosed` as a background service](#running-goosed-as-a-background-service-macos)):

```bash
grep GOOSED_CERT_FINGERPRINT ~/Library/Logs/GooseExternal/goosed.out.log
```

Make a note of the fingerprint — you will paste it into goose Desktop in the next step.

:::note
The fingerprint changes whenever `goosed` regenerates its certificate (for example, if you delete the cert file). If goose Desktop suddenly refuses to connect after a server restart, re-check the fingerprint.
:::

### 4. Configure goose Desktop

On the client machine, open goose Desktop and navigate to **Settings → goose Server**:

| Setting | Value |
|---------|-------|
| **Use external server** | Enabled |
| **URL** | `https://your-server-host:3000` (use the hostname or IP that the client can reach — for example a VPN/tailnet address) |
| **Secret Key** | The same value you used for `GOOSE_SERVER__SECRET_KEY` |
| **Certificate Fingerprint** | The `GOOSED_CERT_FINGERPRINT` value from the server logs |

After saving, goose Desktop will route all backend requests to the remote `goosed`. If the connection fails, see [Troubleshooting](#troubleshooting).

## Running `goosed` as a Background Service (macOS)

Running `goosed` in a terminal session is fine for testing, but for everyday use you probably want it managed as a background service so it starts at login and restarts on failure. On macOS, this is done with `launchd`.

Create a LaunchAgent plist at `~/Library/LaunchAgents/com.goose.goosed.external.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.goose.goosed.external</string>

    <key>ProgramArguments</key>
    <array>
      <string>/Applications/Goose.app/Contents/Resources/bin/goosed</string>
      <string>agent</string>
    </array>

    <key>EnvironmentVariables</key>
    <dict>
      <key>GOOSE_HOST</key><string>0.0.0.0</string>
      <key>GOOSE_PORT</key><string>3000</string>
      <key>GOOSE_TLS</key><string>true</string>
      <key>GOOSE_SERVER__SECRET_KEY</key><string>YOUR_SECRET</string>
    </dict>

    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>

    <key>StandardOutPath</key>
    <string>/Users/YOUR_USERNAME/Library/Logs/GooseExternal/goosed.out.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/YOUR_USERNAME/Library/Logs/GooseExternal/goosed.err.log</string>
  </dict>
</plist>
```

Replace `YOUR_SECRET` and `YOUR_USERNAME` with appropriate values, and make sure the log directory exists:

```bash
mkdir -p ~/Library/Logs/GooseExternal
```

Then load and start the service:

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.goose.goosed.external.plist
launchctl kickstart -k gui/$(id -u)/com.goose.goosed.external
```

To stop or remove it later:

```bash
launchctl bootout gui/$(id -u)/com.goose.goosed.external
```

:::tip
Because the secret key is stored in plain text in the plist, the file should be readable only by your user. macOS LaunchAgents under `~/Library/LaunchAgents/` are already user-scoped, but you can tighten further with `chmod 600 ~/Library/LaunchAgents/com.goose.goosed.external.plist`.
:::

## Troubleshooting

### Server only accepts local connections

If `curl` works from the server but the client machine times out or gets "connection refused", check what interface `goosed` is bound to. If `GOOSE_HOST` is `localhost` or `127.0.0.1`, only loopback connections are accepted.

Set `GOOSE_HOST=0.0.0.0` to accept connections on all interfaces, then restart `goosed`. You can verify with:

```bash
lsof -nP -iTCP:3000 -sTCP:LISTEN
```

The output should show the address as `*:3000` or the specific external IP, not `127.0.0.1:3000`.

### TLS is not enabled

In the server's startup logs:

- If you see `listening on http://...`, TLS is **not** enabled. goose Desktop will not connect. Set `GOOSE_TLS=true` and restart `goosed`.
- If you see `listening on https://...`, TLS is enabled and you are good to go.

The startup logs also contain the `GOOSED_CERT_FINGERPRINT=...` line you need for the goose Desktop configuration. Search the server's stdout (or log file, if running under `launchd`) for `GOOSED_CERT_FINGERPRINT` to find it.

### Client cannot authenticate (401 / Unauthorized)

A `401` from the server, or a goose Desktop error indicating that the secret was rejected, almost always means that `GOOSE_SERVER__SECRET_KEY` on the server does not match the **Secret Key** in goose Desktop's settings.

To check the secret end-to-end without involving goose Desktop, run the authenticated `curl` from [step 2](#2-verify-the-server-is-up) using exactly the value you have configured on the client. If that returns `200`, the secret is correct and the problem is in the client configuration; if it returns `401`, the secret on the server is different from what you are sending.

If you rotate the secret on the server, you must also update it in goose Desktop's settings — they are not synchronized automatically.

### Certificate fingerprint mismatch

If goose Desktop refuses to connect with a certificate or fingerprint error, the most common causes are:

- The server regenerated its certificate (for example, after deleting the cert file). Look at the latest startup logs for the current `GOOSED_CERT_FINGERPRINT` and update goose Desktop.
- You copied the fingerprint with extra whitespace or pasted the wrong value.

## Related

- [Environment Variables](/docs/guides/environment-variables) — full reference for all `GOOSE_*` variables
- [Configuration Files](/docs/guides/config-files) — persistent client-side configuration
