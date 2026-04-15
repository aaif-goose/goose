---
name: create-app-e2e-test
description: Create app e2e tests for the Goose desktop app using the Tauri app test driver. Use when the user wants to generate, write, or verify UI tests that run against the live app.
---

# Create App E2E Test

You are an AI agent that creates app e2e tests for the Goose desktop app using the Tauri app test driver.

## Goal

Given a test scenario in natural language, you will:

1. Explore the app using the test driver CLI to discover what's on screen
2. Write a Vitest test file that verifies the scenario using stable selectors

**Do NOT read source code to understand the UI.** Do not read `.tsx`, `.ts`, or `.css` files to find elements. Use `snapshot` to discover what is on the page — that is your only method. The one exception: read source code only when you need to add a `data-testid` attribute.

## Prerequisites

The Tauri app must already be running in dev mode with the app test driver enabled.

## Test Driver CLI

All exploration commands use the test driver client CLI:

```bash
pnpm test-driver <action> [selector] [value]
```

Available commands:

| Command | Description | Example |
|---------|-------------|---------|
| `snapshot` | Get a text DOM of visible elements | `pnpm test-driver snapshot` |
| `getText <selector>` | Get inner text of an element | `pnpm test-driver getText "h1"` |
| `count <selector>` | Count matching elements | `pnpm test-driver count "button"` |
| `click <selector>` | Click an element | `pnpm test-driver click "button"` |
| `fill <selector> <value>` | Fill an input/textarea | `pnpm test-driver fill "textarea" "hello"` |
| `keypress <selector> <key>` | Dispatch a keyboard event | `pnpm test-driver keypress "textarea" Enter` |
| `waitForText <text>` | Wait for text to appear in body (30s default) | `pnpm test-driver waitForText "Success"` |
| `scroll <direction>` | Scroll the page (up/down/top/bottom) | `pnpm test-driver scroll down` |
| `screenshot [path]` | Take a screenshot | `pnpm test-driver screenshot test.png` |

### Snapshot Format

The `snapshot` command returns a simplified text DOM:

```
[e1] input type="text" placeholder="Ask anything..."
  [t1] label "Name:"
[e2] button "Send"
  [t2] span "Click me"
[t3] h1 "Good afternoon"
```

- `[eN]` — interactive elements (input, button, select, textarea, a) with auto-assigned `data-tid`
- `[tN]` — visible text nodes
- Hidden elements are excluded
- Indentation shows DOM hierarchy

`data-tid` attributes (e.g., `[data-tid='e3']`) are assigned dynamically during each snapshot and **must not** be used in test files — they change between runs.

## Workflow

### Phase 1: Explore

1. Navigate to home first, then `snapshot` to see the current page state.
   ```bash
   pnpm test-driver click '[data-testid="nav-home"]'
   pnpm test-driver snapshot
   ```
   Tests always start from the home screen (`useTestDriver()` navigates home in `beforeEach`), so exploration should too.

2. For each element you need to interact with or verify:
   - Identify it from the snapshot (e.g., `[e3] button "Send"`)
   - Pick a stable selector using the **Element Locating Strategy** below — never use `data-tid`
   - Use `count` with that stable selector to verify it matches exactly one element
   - Use `getText` to verify text content
   - Use `click`/`fill` to perform actions during exploration

3. After each action, run `snapshot` again — the DOM may have changed.

Example exploration session:
```bash
# 1. See what's on the page
pnpm test-driver snapshot

# 2. Pick a selector for the element you need, verify it matches exactly 1
pnpm test-driver count 'textarea[placeholder="Ask anything..."]'

# 3. Interact
pnpm test-driver fill 'textarea[placeholder="Ask anything..."]' "hello world"

# 4. Snapshot again to see the result
pnpm test-driver snapshot
```

### Phase 2: Write the Test

Create a Vitest test file at `tests/app-e2e/<name>.test.ts`.

Use `useTestDriver()` from `tests/app-e2e/lib/setup.ts` to get a shared test driver connection with automatic teardown and screenshot-on-failure. See `tests/app-e2e/chat.test.ts` as a reference:

```typescript
import { describe, it, expect } from "vitest";
import { useTestDriver } from "./lib/setup";

describe("<Feature>", () => {
  const testDriver = useTestDriver();

  it("does something", async () => {
    const text = await testDriver.getText('[data-testid="greeting"]');
    expect(text).toContain("Good");
  });
});
```

Test driver API methods available in tests:

- `testDriver.snapshot()` → text DOM string
- `testDriver.getText(selector, { timeout? })` → inner text string
- `testDriver.count(selector)` → number of matching elements
- `testDriver.click(selector, { timeout? })` → clicks element
- `testDriver.fill(selector, value, { timeout? })` → fills input/textarea
- `testDriver.keypress(selector, key, { timeout? })` → dispatches keyboard event
- `testDriver.waitForText(text, { selector?, timeout? })` → waits for text to appear (default: body, 30s)
- `testDriver.scroll(direction)` → scrolls page ("up", "down", "top", "bottom")
- `testDriver.screenshot(path)` → saves screenshot

All `timeout` options default to 5 seconds. `waitForText` defaults to 30 seconds.

### Phase 3: Verify the Test

Run the test to make sure it passes:

```bash
pnpm test:app-e2e
```

If it fails, use the test driver CLI to re-explore. The issue could be a wrong selector, an incorrect assertion, or a bug in the app implementation.

## Element Locating Strategy

**Always** verify uniqueness with `count` before using any selector in a test. If count > 1, the test will be flaky.

For each element, find a stable selector using this priority:

1. **`data-testid` (preferred)**: if the element already has a `data-testid`, use `'[data-testid="my-element"]'`.
   - Verify with `count` that it's unique

2. **Semantic selector**: combine attributes, hierarchy, or roles to target a specific element.
   - `'input[placeholder="Ask anything..."]'` — narrow by attribute value
   - `'.sidebar [role="navigation"] a'` — narrow by parent context
   - Always verify with `count` that the selector matches exactly 1 element

3. **Add a `data-testid` (last resort)**: if no stable selector exists, add a `data-testid` to the source code.
   - Names must be descriptive and include context (e.g., `home-greeting` not `greeting`, `sidebar-new-chat-button` not `button`)
   - Only add the `data-testid` attribute — do not change any other source code
   - Note the code change so it can be committed alongside the test

**Never** use `data-tid` attributes (`[data-tid='e3']`) in test files — they are assigned dynamically by `snapshot` and change between runs.

## Rules

- One test file per feature area (e.g., `home.test.ts`, `sidebar.test.ts`, `settings.test.ts`)
- Keep test descriptions specific: "shows time-based greeting on home screen", not "home works"
- Always check `count` === 1 for selectors before using them in assertions
- Do not use `snapshot` in test assertions — it's for exploration only
- The test driver automatically waits up to 5 seconds (configurable via `{ timeout }`) for elements to appear before `click`, `fill`, `getText`, and `keypress`
- Use `waitForText` to wait for specific text content to appear (e.g., after submitting a form or waiting for an LLM response)
- If the DOM updates after an action, run `snapshot` again to see the new state before writing assertions
- Do not add comments in test files — the test descriptions and code should be self-explanatory
