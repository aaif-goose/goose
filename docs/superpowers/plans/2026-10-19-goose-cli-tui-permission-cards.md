# Goose CLI TUI Permission Cards Plan

> **For agentic workers:** Execute the tasks in order. Keep Goose's existing permission policy authoritative.

**Goal:** Present interactive tool approval requests in a readable keyboard-driven Goose TUI card.

**Architecture:** Reuse the existing `ToolConfirmationRequest`, `Permission` options, and submission flow. When an interactive terminal receives an approval request, briefly enter a Ratatui permission screen, return the selected existing permission value, then let the current agent stream continue. Headless behavior and `goose run` output stay unchanged.

**Design constraints:**

- Display the requested tool, its arguments, and any policy prompt without truncating away the action under review.
- Offer Allow once, Deny, and Cancel; offer Always allow only when the request has no custom prompt, matching current behavior.
- Start with Deny selected so Enter cannot implicitly authorize a tool. Escape maps to the existing Cancel value.
- Use the existing `Permission` enum and `submit_tool_confirmation`; do not bypass or reinterpret session policy.
- Restore terminal raw mode and alternate-screen state on every exit path.
- Keep current cliclack prompt available as the fallback if the TUI cannot initialize.

## Tasks

### Task 1: Define permission-card state and presentation

- Add unit tests for available choices, the default deny selection, navigation bounds/wrapping, Enter selection, and Escape cancellation.
- Run focused tests and confirm the new permission-card state is missing.
- Implement a small typed state and a high-contrast Ratatui card with direct language and keyboard hints.
- Verify narrow and no-color rendering through buffer tests.
- Commit as `feat: add TUI permission approval cards`.

### Task 2: Route interactive tool approvals through the card

- Add integration-level tests around the request-to-choice mapping, including custom prompts excluding Always allow.
- Run the focused tests and confirm the current interactive path still calls cliclack.
- Use the TUI card for interactive requests, retaining cliclack as an initialization fallback and leaving headless policy unchanged.
- Run focused tests, `cargo check -p goose-cli`, and `GOOSE_DISABLE_KEYRING=1 cargo test -p goose-cli --lib`.
- Commit as `feat: use TUI for interactive tool approvals`.

## Verification

- Run `cargo fmt --all` and `git diff --check`.
- Confirm Escape submits `Permission::Cancel`, explicit Deny submits `Permission::DenyOnce`, and only the no-custom-prompt card includes `Permission::AlwaysAllow`.
- Confirm `goose run` and non-interactive approval behavior are unchanged.
