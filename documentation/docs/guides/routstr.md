---
sidebar_position: 50
title: Routstr (pay-per-request via Cashu)
sidebar_label: Routstr
description: Pay for LLM requests in Bitcoin sats through any Routstr proxy, with a local Cashu wallet
---

# Routstr

[Routstr](https://routstr.com/docs) is an OpenAI-compatible LLM proxy that
bills per request in Bitcoin sats via [Cashu](https://cashu.space/) ecash
tokens. goose ships:

- A `routstr` provider that talks to any Routstr instance.
- A local Cashu wallet (`goose wallet`) that holds your sats. Built on
  [`cdk`](https://github.com/cashubtc/cdk), the Rust Cashu development kit.
- A `goose configure → Configure Providers → Routstr` flow that wires
  everything together — picking a Routstr URL, exchanging some local sats
  for a per-host `sk-...` API key, and refunding the proxy back to your
  wallet when you switch hosts.

## Two layers of state

There are two places sats can live:

1. **Local Cashu wallet** at `~/.cdk-gooose/`. One BIP-39 seed
   (`~/.cdk-gooose/seed`), one redb proof store
   (`~/.cdk-gooose/cdk-goose.redb`), one mint
   (`https://mint.minibits.cash/Bitcoin`, hardcoded). This is your
   source of truth for sats. Funded with `goose wallet topup
   <cashu-token>`.

2. **Per-profile balance on a Routstr instance**, identified by an
   `sk-...` API key the proxy issues in exchange for a Cashu token. Each
   profile is `{url, api_key}` stored in
   `~/.config/goose/config.yaml` under `ROUTSTR_PROFILES.<name>`. The
   active profile is named by `ROUTSTR_ACTIVE`.

`goose configure → Routstr → URL` moves sats between these two layers
automatically (refund the old profile, fund the new one) — you don't
need to touch `ROUTSTR_PROFILES` by hand.

## Quick start

```bash
# 1. Receive a Cashu token into the local wallet. Get the token from any
#    wallet minting against Minibits (the Minibits app, cashu.me, etc.).
goose wallet topup cashuB...
goose wallet balance
# → local wallet: 2000 sats

# 2. Pick a Routstr instance and a model.
goose configure
# → Configure Providers → Routstr
# → Routstr URL: https://api.routstr.com   (or any Routstr instance)
# → pick a model from the proxy's catalogue

# 3. Chat.
goose run --provider routstr --model anthropic/claude-sonnet-4 \
  --text "Hi from a paid Cashu wallet"
```

The configure URL prompt:

- **Same URL as the active profile, no api_key yet** — auto-funds the
  active profile from the local wallet (default 2000 sats, capped at
  your local balance).
- **URL matches a different existing profile** — switches to it: refund
  the previously active profile back into the local wallet, auto-fund
  the new one.
- **New URL** — refunds the previously active profile, creates a profile
  named `default` pointing at the new URL, makes it active, and auto-funds
  it.

After the URL prompt, configure fetches `<active-url>/v1/models` and
presents an interactive picker against every model the proxy serves.
Type to filter (`claude`, `gemini`, `llama`, …).

## Refilling and switching

```bash
# Refill the local wallet, then re-fund the active profile:
goose wallet topup <new-cashu-token>
goose configure   # → Routstr → same URL → auto-fund

# Switch hosts (refunds the old profile, funds the new one):
goose configure   # → Routstr → <new URL>

# Drain the local wallet to a Cashu token (e.g. to migrate machines):
goose wallet withdraw [N]
```

`goose wallet balance/topup/withdraw` also drain a queue of failed
refunds the next time they run (see [Offline refunds](#offline-refunds)).

## Insufficient balance

If a chat request returns
`Insufficient balance: <N> sats required`, that's the model's per-request
**reservation minimum** on the proxy — not your debt. The model needs
`<N>` sats reserved up-front to start the request; refund what's left
after the response. Some models (e.g. `gpt-5.5-openai`) reserve a few
thousand sats per call.

Top up to at least that much:

```bash
goose wallet topup cashuB...      # local
goose configure   # → Routstr → same URL  → auto-tops the active profile
```

…or pick a cheaper model in the picker (`claude-haiku-4.5`,
`glm-5.1`, `gemini-3.1-flash-lite-preview`).

## Offline refunds

When `goose configure → Routstr → <new URL>` tries to refund the
previously active profile and the proxy is unreachable (offline,
rate-limited, etc.), the `(url, api_key)` pair is appended to
`~/.cdk-gooose/pending-refunds.json` instead of being lost.

The next `goose wallet balance/topup/withdraw` retries every queued
refund, removes successes, keeps failures for next time:

```text
$ goose wallet topup cashuB...
↻ draining 1 pending Routstr refund(s)…
  ✓ refunded 976 sats from https://routstr.otrta.me
Received 1000 sats. Local wallet balance: 1976 sats.
```

You don't need to do anything special — just run any wallet command
after the proxy comes back.

## Where things live

| Path | Holds |
| --- | --- |
| `~/.cdk-gooose/seed` | BIP-39 mnemonic for the local wallet. **Back this up.** |
| `~/.cdk-gooose/cdk-goose.redb` | Local Cashu proofs. |
| `~/.cdk-gooose/pending-refunds.json` | Queue of `(url, sk-...)` pairs whose refund POST failed. Drained on the next `goose wallet *` call. |
| `~/.config/goose/config.yaml` (`ROUTSTR_PROFILES`) | Per-profile `{url, api_key}` map. The `api_key` is `sk-...` issued by the proxy, not a Cashu token. |
| `~/.config/goose/config.yaml` (`ROUTSTR_ACTIVE`) | Active profile name. |

## Limitations

- **Minibits is the only supported mint** today — `https://mint.minibits.cash/Bitcoin`,
  hardcoded. Tokens minted at any other mint won't receive into the
  local wallet, and Routstr instances trusting a different mint will
  reject keys created with our wallet's tokens.
- **`sk-...` keys are stored in plaintext** in
  `~/.config/goose/config.yaml`, not the OS keychain. The seed at
  `~/.cdk-gooose/seed` is similarly plaintext. File permissions are
  your only barrier on a shared machine.
- **Tool-use coverage varies by model.** The configure flow's final
  test step calls the picked model with a tool spec; some models in the
  large proxy catalogue (e.g. `aion-1.0`) don't accept `tools=[]` and
  the test fails. Re-run configure and pick a tool-aware model — the
  Claude / Gemini / GLM / GPT-5 families are all known to work.
