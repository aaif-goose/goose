# New model check: agent instructions

You check whether goose handles newly launched models correctly, and report the results in the channel. Follow these steps exactly. Don't add checks, and don't skip any.

You never call a real model. You use a fake API key: the provider rejects the request, but goose has already written the exact request to its log, and that request is what you check.

If a setup command fails (the script, the download, the install), post the error and stop. The goose runs in step 4 are expected to end in an error; that's not a failure.

## 1. Find new models

From a goose checkout, run:

```
node scripts/new-model-check/check_new_models.mjs > new_models.json
```

- Exit code 0: nothing new. Stop; post nothing.
- Exit code 10: `new_models` lists the new models with their models.dev metadata.
- Exit code 1: the script failed.

## 2. Set up

Use a new, empty folder, and give goose its own home folder, so goose has no extensions or settings and nothing from this machine ends up in the requests.

```
W=$(mktemp -d)
mkdir -p "$W/bin" "$W/home" "$W/work"
cd "$W/work"
curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh -o "$W/download_cli.sh"
HOME="$W/home" GOOSE_BIN_DIR="$W/bin" CONFIGURE=false bash "$W/download_cli.sh"
"$W/bin/goose" --version
R="$W/results.md"
```

Record results in `$R` as you go, never at the end. Write the goose version as its first line. After every check, write one line: model, release or canary, setting, check, value sent, value expected, result. Write notes and the canary version there too.

Define this helper. Let goose finish every run: don't pipe its output into anything that stops reading early, like `head`, or goose is cut off before it sends the real request.

```
run() {
  rm -f "$W/home/.local/state/goose/logs/"llm_request.*.jsonl
  env HOME="$W/home" GOOSE_DISABLE_KEYRING=1 "$@" \
    timeout 120 "$W/bin/goose" run --text "hi" --no-session > "$W/last_run.txt" 2>&1
}
```

Then do one warm-up run with any new model (settings in step 3) and ignore the result. On its first run, goose hasn't downloaded models.dev yet and falls back to a 4096 output limit.

## 3. Provider settings

| models.dev provider | `GOOSE_PROVIDER` | Fake key variable |
|---|---|---|
| `anthropic` | `anthropic` | `ANTHROPIC_API_KEY` |
| `openai` | `openai` | `OPENAI_API_KEY` |
| `google` | `google` | `GOOGLE_API_KEY` |
| `xai` | `xai` | `XAI_API_KEY` |
| `zai` | `zai` | `ZHIPU_API_KEY` |
| `moonshotai` | `moonshot` | `MOONSHOT_API_KEY` |
| `deepseek` | `custom_deepseek` | `DEEPSEEK_API_KEY` |

Set the fake key to `fake-key`.

## 4. Run goose and collect requests

For each new model, run once per thinking setting: `off`, `low`, `medium`, `high` and `max`.

```
run GOOSE_PROVIDER=<provider> GOOSE_MODEL=<model> <KEY_VARIABLE>=fake-key GOOSE_THINKING_EFFORT=<setting>
```

After each run, find the real request in `$W/home/.local/state/goose/logs/llm_request.*.jsonl`. Each file's first line is a JSON object with `model_config` and `input`; `input` is the exact request goose sent. Each run usually logs two requests:
- **The real request:** `model_config.request_params.thinking_effort` matches the setting you ran, and there's no `disable_prompt_cache`.
- **goose's background request** to name the session: it has `disable_prompt_cache: true`. Ignore it.

If there's no real request, report that for this model and move on.

## 5. Where the settings are in the request

| Format | Output limit | Thinking |
|---|---|---|
| Google | `generationConfig.maxOutputTokens` | `generationConfig.thinkingConfig`: `thinkingLevel` (a level) or `thinkingBudget` (a token budget) |
| Anthropic | `max_tokens` | `thinking.type` (`adaptive`, `enabled` or `disabled`), `thinking.budget_tokens` (a token budget), `output_config.effort` (a level) |
| OpenAI (Responses API) | `max_output_tokens` | `reasoning.effort` (a level) |
| OpenAI (Chat API) | `max_completion_tokens` or `max_tokens` | `reasoning_effort` (a level) |
## 6. The checks

Compare the real requests with the model's models.dev metadata.

1. **Output limit.** The max-setting request asks for `output_limit` from models.dev. A request for 4096 when models.dev says otherwise is a fail. If models.dev has no output limit, or it equals the context limit, goose ignores it by design: write "skipped".
2. **Thinking is on.** If models.dev says `reasoning: true`, the max-setting request includes thinking settings. No thinking fields at all is a fail.
3. **Thinking levels.** If models.dev's `reasoning_options` has an entry with `"type": "effort"`, its `values` are the model's levels, lowest to highest. Then:
   - For `low`, `medium`, `high` and `max`, the request must use one of those levels, not a token budget (`thinkingBudget`, `budget_tokens`). A token budget is a fail, even though thinking is on.
   - Every level goose sends must be on the list.
   - `max` must send the last (top) value on the list.
   - For `off`, pass if thinking is disabled, left out, or set to a level on the list. Some models can't turn thinking off.

   If models.dev has no levels for the model, find them in the provider's launch notes or API docs. If you can't, write "levels unknown" rather than pass or fail.

Quote the actual value from the request for every check (for example `thinkingLevel: "xhigh"`). Never paste whole requests or logs.

## 7. Read the provider's notes

Providers also introduce new rules that models.dev doesn't capture. For each new model, read the provider's launch post for the model, and the provider's changelog entries from the last 7 days:

| Provider | Changelog |
|---|---|
| Anthropic | https://platform.claude.com/docs/en/release-notes/overview |
| OpenAI | https://developers.openai.com/api/docs/changelog |
| Google | https://aistudio.google.com/docs/changelog |
| xAI | https://docs.x.ai/developers/release-notes |
| Zhipu | https://docs.z.ai/release-notes/new-released |
| Moonshot | No API changelog found; use https://platform.kimi.ai/blog |
| DeepSeek | https://api-docs.deepseek.com/updates/ |

Only report changes that could affect what goose sends or receives, such as thinking settings, output limits, tool calling, streaming, headers or removed settings.

For each item, give a one-line summary, what it means for goose, a short quote and the link. These don't affect PASS or FAIL. If a person should look at one, add it to "Action needed" in the report.

If a change clearly breaks goose, follow the PR rules in step 8 ("Canary fails too").

## 8. If a check fails

Install the canary build, which is built from main, into another fresh folder, and point the `run` helper at it:

```
C=$(mktemp -d); mkdir -p "$C/bin" "$C/home" "$C/work"
HOME="$C/home" GOOSE_BIN_DIR="$C/bin" CANARY=true CONFIGURE=false bash "$W/download_cli.sh"
W=$C
cd "$W/work"
```

Do the warm-up run, then repeat the failing checks.

- **Canary passes:** the fix is on main. Name the PR if you can find it. If `gh release list -R aaif-goose/goose -L 1` shows a newer release than the one you tested and it includes the fix, report that no action is needed, because stable will catch up soon. Otherwise, report that a patch release is needed.
- **Canary fails too:** in your goose checkout, on the latest main, read that provider's request code and find why. Report the file and line, and describe the fix.

  Then open a PR with the fix, but only if all of these hold:
  - No open PR already fixes it. Search open PRs for the model and the provider first; if one exists, link it in the report instead.
  - The cause is clear, and the code change only touches that provider's code.
  - The PR includes a test that fails before the fix and passes after.

  Link the PR in the report. If any condition doesn't hold, report your findings without opening a PR.

## 9. Report

Build the report only from `$R`, not from memory. Post one message per run, covering every new model you checked:

```
New model check: 2026-11-20

**Action needed:**
- Look at note 1 (GPT-6.2 reasoning items).

**New models:**
- FAIL google/gemini-4-neon: on 1.60.1, fixed in 1.60.2 (#13673). **No action.**
  - Thinking is on: sent no `thinkingConfig`, models.dev `reasoning: true`
  - Thinking levels: max sent nothing, expected `thinkingLevel: "xhigh"`
- PASS openai/gpt-6.2-astra
- PASS openai/gpt-6.2-sol

**Notes:**
1. GPT-6.2 drops reasoning items when history is edited. May affect compaction. "<short quote>" <link>
2. GPT-6.2 tool calling needs the Responses API. goose already uses it. "<short quote>" <link>

Tested goose 1.60.1 (main 1.60.0-canary+42055e7)
```

- **Action needed** comes first. List every action, or write "None". An action is one of: a patch release, review a PR, a fix with no PR (say why), or look at a note.
- **New models:** one PASS or FAIL line per model. End a FAIL line with its action in bold: **No action.**, **Patch release.**, **Review PR <link>.** or **Needs a fix.** List each failed check below it, with the value sent and the value expected, and the cause's file and line if main fails too.
- **Notes** are information only. Their actions go in "Action needed".
- End with the goose version tested, and the canary version if you used it.
