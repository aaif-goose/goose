# SessionListView Token-Count Inconsistency

Two list-style session views answer the same UX question — "how many tokens did this session use?" — and pick different fields.

## The two sites

`ui/desktop/src/components/sessions/SessionListView.tsx:780-784`:

```tsx
{session.total_tokens !== null && (
  <div className="flex items-center">
    <Target className="w-3 h-3 mr-1" />
    <span className="font-mono">{(session.total_tokens || 0).toLocaleString()}</span>
  </div>
)}
```

`ui/desktop/src/components/schedule/ScheduleDetailView.tsx:527-531`:

```tsx
{session.accumulatedTotalTokens !== undefined &&
  session.accumulatedTotalTokens !== null && (
    <p>{intl.formatMessage(i18n.tokens, { count: session.accumulatedTotalTokens })}</p>
  )}
```

## Why this is inconsistent

`total_tokens` is per-call (overwritten on every provider call to `input + output` of that one call). `accumulated_total_tokens` is the running sum across every provider call in the session.

A 30-turn session where each turn used ~10k tokens but ended on a small "Done." ack:

| Field | Value | Meaningful? |
|---|---|---|
| `total_tokens` | ~1,200 | ✗ misleading |
| `accumulated_total_tokens` | ~300,000 | ✓ matches reality |

`ScheduleDetailView` shows the meaningful number. `SessionListView` shows the misleading one.

There's also a shape inconsistency: `SessionListView` reads `session.total_tokens` (snake_case from the REST `Session` type), `ScheduleDetailView` reads `session.accumulatedTotalTokens` (camelCase from the schedule API's separate session shape — `ui/desktop/src/schedule.ts:33-37`).

## Recommended fix

Switch `SessionListView` to render `session.accumulated_total_tokens` to match `ScheduleDetailView`'s semantics:

```tsx
{session.accumulated_total_tokens != null && (
  <div className="flex items-center">
    <Target className="w-3 h-3 mr-1" />
    <span className="font-mono">{session.accumulated_total_tokens.toLocaleString()}</span>
  </div>
)}
```

The field already exists on the REST `Session` type (`crates/goose/src/session/session_manager.rs:72`) — it just isn't referenced in this view.

## Caveat

This will make every existing user's numbers jump, often 10–50×. Worth calling out in the PR description so it isn't read as a regression.

## What this is *not*

Don't confuse "tokens spent in this session" (this doc) with "context-window fill %" (a different metric).

- **Tokens spent in this session** → `accumulated_total_tokens`. Cumulative, lifetime quantity.
- **Context-window fill %** → `total_tokens / context_limit`. Snapshot, transient quantity. Stays in the live chat UI (`ChatInput.tsx:96-97`, fed by `tokenState.totalTokens`).

The two are different UX elements answering different questions and should not be merged into one number.
