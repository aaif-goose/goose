---
title: macOS Sandbox for goose Desktop
sidebar_label: Sandbox for goose Desktop
description: Optional sandboxing for goose Desktop to control file access, filter network traffic, and enforce security policies on macOS
---

goose Desktop includes an optional macOS sandbox that you can enable when you need stricter control and visibility over what goose can access on your system. Use it to:

- **Restrict file system access** — Block writes to SSH keys, shell configs, and goose configuration files
- **Control network connections** — Force all traffic through a filtering proxy that blocks unapproved domains
- **Prevent security bypasses** — Block tunneling tools, raw sockets, and other techniques that could circumvent restrictions
- **Audit and enforce policies** — Log all network activity and enforce compliance requirements

goose runs with full tool access, but the sandbox uses two layers of protection:
- **File access control** - Apple's `sandbox-exec` restricts file and network access at the system level
- **Outbound connections** - A local egress proxy filters and logs outgoing connections

:::info macOS Requirement
The sandbox relies on `/usr/bin/sandbox-exec`, which is only available on macOS and is also known as Apple's seatbelt technology.
:::

## Quick Start

To enable the sandbox, launch goose Desktop from the terminal with the environment variable set. For example:

```bash
export GOOSE_SANDBOX=true
open -a Goose
```

When the app starts with sandboxing enabled, it will:

1. Generate a seatbelt sandbox profile
2. Start a local HTTP CONNECT proxy on localhost
3. Launch the `goosed` backend for goose Desktop inside `sandbox-exec`, forcing all traffic through the proxy

The sandbox remains active until you quit goose Desktop. To disable it, quit the app and relaunch normally (or set `GOOSE_SANDBOX=false` when opening from the terminal).

## Configuration

All configuration is via environment variables. Defaults are designed to be secure out of the box, but you can adjust them to match your security requirements.

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `GOOSE_SANDBOX` | `false` | Set to `true` or `1` to enable the sandbox. See [Quick Start](#quick-start) for launch instructions. |

----

### File System

The [seatbelt sandbox profile](https://github.com/aaif-goose/goose/blob/main/ui/desktop/src/sandbox/index.ts) blocks write operations to these sensitive files:

- `~/.ssh/` - Prevent SSH key tampering
- `~/.bashrc`, `~/.zshrc`, `~/.bash_profile`, `~/.zprofile` - Prevent shell config injection
- `~/.config/goose/sandbox/` - Protect sandbox config from the sandboxed process
- `~/.config/goose/config.yaml` - Protect goose config


#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GOOSE_SANDBOX_PROTECT_FILES` | `true` | Write-protect sensitive files listed above. Set to `false` to disable |

----

### Direct Network Access

The seatbelt sandbox denies all direct network access, forcing traffic through the proxy. The only allowed connections are:

- **Localhost** — Allows the `goosed` process to reach the egress proxy and its own server port
- **Unix sockets** — For local inter-process communication (IPC)
- **mDNSResponder** — For DNS resolution

:::info Not Configurable
These restrictions are always active when the sandbox is enabled.
:::

----

### Process Restrictions

The seatbelt sandbox blocks tools and system calls that could bypass security controls:

- **Tunneling tools** — `nc`, `ncat`, `netcat`, `socat`, `telnet` are blocked to prevent bypassing the proxy
- **Raw sockets** — `SOCK_RAW` on `AF_INET`/`AF_INET6` is blocked to prevent raw packet crafting
- **Kernel extensions** — `system-kext-load` is denied

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GOOSE_SANDBOX_BLOCK_RAW_SOCKETS` | `true` | Block `SOCK_RAW`. Set to `false` to disable |
| `GOOSE_SANDBOX_BLOCK_TUNNELING` | `true` | Block `nc`/`netcat`/`socat`/`telnet`. Set to `false` to disable |

----

### Network Filtering

The egress proxy inspects and filters all outgoing connections. You can customize filtering rules through the blocklist file and configuration variables.

The egress proxy checks connections in this order:

1. **Loopback detection** — Prevents using the proxy as a relay back to localhost
2. **Raw IP blocking** — Connections to bare IP addresses (no domain) are blocked
3. **Domain blocklist** — Domains listed in `blocked.txt` are denied (including all subdomains)
4. **SSH/Git host restrictions** — SSH ports (22, 2222, 7999) are restricted to known git hosts

For optional LaunchDarkly-based egress control, see [LaunchDarkly](#launchdarkly-optional).

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GOOSE_SANDBOX_ALLOW_IP` | `false` | Set to `true` to allow connections to raw IP addresses |
| `GOOSE_SANDBOX_BLOCK_LOOPBACK` | `false` | Set to `true` to block loopback relay through the proxy |
| `GOOSE_SANDBOX_ALLOW_SSH` | `true` | Set to `false` to block all SSH traffic |
| `GOOSE_SANDBOX_GIT_HOSTS` | built-in list | Comma-separated list of allowed SSH git hosts (e.g. `github.com,gitlab.com`) |
| `GOOSE_SANDBOX_SSH_ALL_HOSTS` | `false` | Set to `true` to allow SSH to any host (not just git hosts) |

#### Managing the Domain Blocklist

The file `~/.config/goose/sandbox/blocked.txt` controls which domains are blocked by the proxy. It's created automatically on first run from a bundled template.

```
# One domain per line. Subdomains are blocked automatically.
# Lines starting with # are comments.
evil.com          # blocks evil.com and *.evil.com
pastebin.com
transfer.sh
webhook.site
```

:::tip Live Reload
Changes to `blocked.txt` take effect immediately — the proxy watches the file with `fs.watch` and reloads it automatically. No restart needed.
:::

#### Using Git Over SSH

SSH git operations (e.g. `git clone git@github.com:...`) work through the sandbox via a bundled `connect-proxy.pl` script that acts as an SSH `ProxyCommand`. This routes SSH connections through the egress proxy, which then applies the same allowlist rules.

By default, SSH is only allowed to well-known git hosting domains (e.g. GitHub, GitLab, Bitbucket). To customize:

```bash
# Add custom git hosts
export GOOSE_SANDBOX_GIT_HOSTS="github.com,gitlab.com,your-gitea.internal.com"

# Or allow SSH to all hosts
export GOOSE_SANDBOX_SSH_ALL_HOSTS=true
```

----

### LaunchDarkly (Optional)

For enterprise environments, LaunchDarkly provides optional dynamic egress control. If not configured, the sandbox uses the local `blocked.txt` blocklist.

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `LAUNCHDARKLY_CLIENT_ID` | — | LD client SDK key to enable dynamic egress control |
| `GOOSE_SANDBOX_LD_FAILOVER` | — | Failover mode if LD is unreachable: `allow`, `deny`, or `blocklist` |

## Example Configurations

### Maximum security

```bash
export GOOSE_SANDBOX=true
# All protections enabled (defaults)
```

### Allow raw IP connections (e.g. for internal APIs)

```bash
export GOOSE_SANDBOX=true
export GOOSE_SANDBOX_ALLOW_IP=true
```

### Disable SSH entirely

```bash
export GOOSE_SANDBOX=true
export GOOSE_SANDBOX_ALLOW_SSH=false
```

### Relaxed mode (fewer restrictions)

```bash
export GOOSE_SANDBOX=true
export GOOSE_SANDBOX_PROTECT_FILES=false
export GOOSE_SANDBOX_BLOCK_RAW_SOCKETS=false
export GOOSE_SANDBOX_BLOCK_TUNNELING=false
export GOOSE_SANDBOX_ALLOW_IP=true
export GOOSE_SANDBOX_SSH_ALL_HOSTS=true
```

### With LaunchDarkly egress control

```bash
export GOOSE_SANDBOX=true
export LAUNCHDARKLY_CLIENT_ID=sdk-your-key-here
export GOOSE_SANDBOX_LD_FAILOVER=blocklist  # fall back to local blocklist if LD is down
```

## Troubleshooting

- **Error: "GOOSE_SANDBOX=true but sandbox-exec is not available (macOS only)"**  
  You're not on macOS, or `/usr/bin/sandbox-exec` is missing. The sandbox only works on macOS.

- **Extensions or tools can't reach the network**  
  Check if the destination domain is in `~/.config/goose/sandbox/blocked.txt`, or if you need to enable `GOOSE_SANDBOX_ALLOW_IP=true` for IP-based endpoints.

- **git clone over SSH fails**  
  The target host may not be in the default Git hosts allowlist. Add it with `GOOSE_SANDBOX_GIT_HOSTS=your-host.com` or set `GOOSE_SANDBOX_SSH_ALL_HOSTS=true`.

- **Want to inspect what the proxy is blocking?**  
  Check the [Desktop application logs](/docs/guides/logs#desktop-application-log). Blocked connections are logged with the prefix `[sandbox-proxy]` and include the reason for blocking.

