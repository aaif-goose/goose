# New model check: agent instructions

You check whether goose handles newly launched models correctly, and report the results in the channel. Follow these steps exactly. Don't add checks, and don't skip any.

You never call a real model. You use a fake API key: the provider rejects the request, but goose has already written the exact request to its log, and that request is what you check.

If a setup command fails (the script, the download, the install), post the error and stop. The goose runs in step 4 are expected to end in an error; that's not a failure.

## 1. Find new models

From a goose checkout, get the models.dev data:

```
START=$(date -u '+%Y-%m-%d %H:%M UTC')
S="$PWD/scripts/new-model-check"
CATALOG="$(mktemp -d)/models_dev_api.json"
scripts/new-model-check/download_models_dev.sh "$CATALOG"
```

If the download fails (some sandboxes block models.dev), build the same file from the models.dev repository instead. The script prints the models.dev commit it built from; keep it for the report.

```
scripts/new-model-check/build_models_dev.sh "$CATALOG"
```

If both fail, post both errors and stop. Once you have the file, run:

```
node scripts/new-model-check/check_new_models.mjs "$CATALOG" > new_models.json
```

- Exit code 0: nothing new. Post `New model check: $START. No new models in the last 3 days.` and stop.
- Exit code 10: new models found. `new_models` in `new_models.json` lists them with their models.dev metadata. Continue with step 2.
- Exit code 1: the script failed. Post the error and stop.

## 2. Set up

Use a new, empty folder, and give goose its own folders, so goose has no extensions or settings and nothing from this machine ends up in the requests.

```
W=$(mktemp -d)
mkdir -p "$W/bin" "$W/home" "$W/work" "$W/requests"
cd "$W/work"
curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh -o "$W/download_cli.sh"
HOME="$W/home" GOOSE_BIN_DIR="$W/bin" GOOSE_VERSION= CANARY=false CONFIGURE=false bash "$W/download_cli.sh"
"$W/bin/goose" --version
R="$W/results.md"
```

Record results in `$R` as you go, never at the end. Write the goose version as its first line, and where the models.dev data came from as the second: downloaded, or built from commit `<sha>`. After every check, write one line: model, release or canary, setting, check, value sent, value expected, result. Write notes and the canary version there too.

Then give goose the models.dev data from step 1. Without it, goose has only its built-in model list, which doesn't know the new models, so it uses a 4096 output limit for them. goose still tries to download models.dev itself; where that's blocked, it keeps this copy.

```
mkdir -p "$W/root/data/model_catalog"
cp "$CATALOG" "$W/root/data/model_catalog/models_dev_api.json"
```

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
"$S/run_goose.sh" "$W" GOOSE_PROVIDER=<provider> GOOSE_MODEL=<model> <KEY_VARIABLE>=fake-key GOOSE_THINKING_EFFORT=<setting>
```

The script starts goose without your environment, except `PATH`, temp and proxy settings, keeps all of goose's files under `$W/root`, and writes goose's output to `$W/last_run.txt`. Let every run finish.

After each run, find the real request in `$W/root/state/logs/llm_request.*.jsonl`. Each file's first line is a JSON object with `model_config` and `input`; `input` is the exact request goose sent. Each run usually logs two requests:
- **The real request:** `model_config.request_params.thinking_effort` matches the setting you ran, and there's no `disable_prompt_cache`.
- **goose's background request** to name the session: it has `disable_prompt_cache: true`. Ignore it.

Copy the real request's file to `$W/requests/<model>-<setting>.jsonl`; the next run deletes the logs.

If there's no real request, that setting is a FAIL: record "no request sent" and the error line from `$W/last_run.txt`.

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
2. **Thinking is on.** If models.dev says `reasoning: true`, the max-setting request includes thinking settings; none at all is a fail. Exception: if `reasoning_options` is `[]`, the model always reasons and takes no settings, so the request must have none.
3. **Thinking levels.** If models.dev's `reasoning_options` has an entry with `"type": "effort"`, its `values` are the model's levels, lowest to highest. Then:
   - For `low`, `medium`, `high` and `max`, the request must use one of those levels, not a token budget (`thinkingBudget`, `budget_tokens`). A token budget is a fail, even though thinking is on.
   - Every level goose sends must be on the list.
   - `max` must send the last (top) value on the list.
   - For `off`, pass if thinking is disabled, left out, or set to a level on the list. Some models can't turn thinking off.

   If models.dev has no levels for the model, find them in the provider's launch notes or API docs. If you can't, write "levels unknown" rather than pass or fail.

Quote the actual value from the request for every check (for example `thinkingLevel: "xhigh"`). Never paste whole requests or logs.

Before moving on, recheck:
- **Each FAIL:** rerun that setting once. If the rerun passes, use the rerun's result and note in `$R` that the first run failed.
- **Each PASS:** don't rerun goose. Reopen the saved request and confirm it's the real request, it contains the quoted value, and the expected value matches `new_models.json`.

Fix `$R` where anything doesn't match.

## 7. Read the provider's notes

Providers also introduce new rules that models.dev doesn't capture. For each new model, read the provider's launch post for the model, and the provider's changelog entries from the last 3 days:

| Provider | Changelog |
|---|---|
| Anthropic | https://platform.claude.com/docs/en/release-notes/overview |
| OpenAI | https://developers.openai.com/api/docs/changelog |
| Google | https://aistudio.google.com/docs/changelog |
| xAI | https://docs.x.ai/developers/release-notes |
| Zhipu | https://docs.z.ai/release-notes/new-released |
| Moonshot | No API changelog found; use https://platform.kimi.ai/blog |
| DeepSeek | https://api-docs.deepseek.com/updates/ |

Only report changes that could affect what goose sends or receives, such as thinking settings, output limits, tool calling, streaming, prompt caching, headers or removed settings.

In the provider's notes, for prompt caching, report any change to: whether the model caches automatically or needs cache breakpoints (`cache_control`), the maximum number of breakpoints (goose uses 4), the lookback window (goose assumes 20 blocks), the minimum cacheable length, or the cache lifetime (TTL). goose's caching rules start from `crates/oi-kit-provider-types/src/cache_semantics.rs`.

For each item, give a one-line summary, what it means for goose, a short quote and the link. End each with a bold action: `**Action: none.**` or `**Action: <what to do>.**` These don't affect PASS or FAIL.

If a change clearly breaks goose, follow the PR rules in step 8 ("Canary fails too").

## 8. If a check fails

Do this once, after every new model has been through steps 4–7, for all failing checks together.

Install the canary build, which is built from main, into another fresh folder, and use it for the reruns:

```
C=$(mktemp -d); mkdir -p "$C/bin" "$C/home" "$C/work" "$C/requests"
HOME="$C/home" GOOSE_BIN_DIR="$C/bin" GOOSE_VERSION= CANARY=true CONFIGURE=false bash "$W/download_cli.sh"
W=$C
```

Copy the models.dev data into this folder as in step 2, then repeat the failing checks.

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
New model check: 2026-11-20 09:53 UTC

**Action needed:**
- Look at note 1 (GPT-6.2 reasoning items).

**New models:**
- FAIL google/gemini-4-neon: on 1.60.1, fixed in 1.60.2 (#13673). **No action.**
  - Thinking is on: sent no `thinkingConfig`, models.dev `reasoning: true`
  - Thinking levels: max sent nothing, expected `thinkingLevel: "xhigh"`
- PASS openai/gpt-6.2-astra
- PASS openai/gpt-6.2-sol

**Notes:**
1. GPT-6.2 drops reasoning items when history is edited. May affect compaction. "<short quote>" <link> **Action: check whether compaction keeps reasoning items.**
2. GPT-6.2 tool calling needs the Responses API. goose already uses it. "<short quote>" <link> **Action: none.**

Tested goose 1.60.1 (main 1.60.0-canary+42055e7)
```

- The header has `$START` from step 1.
- **Action needed** comes first. List every action, or write "None". An action is one of: a patch release, review a PR, a fix with no PR (say why), or look at a note.
- **New models:** one PASS or FAIL line per model. A model with no FAIL is PASS; list any skipped or levels-unknown check below it. End a FAIL line with its action in bold: **No action.**, **Patch release.**, **Review PR <link>.** or **Needs a fix.** List each failed check below it, with the value sent and the value expected, and the cause's file and line if main fails too.
- **Notes:** end each with its action. List every note whose action isn't "none" in "Action needed".
- End with the goose version tested, and the canary version if you used it. If you built the models.dev data, add "models.dev data built from <sha>".
- Before posting, check the report: every model in `new_models.json` is listed, every FAIL line and note ends with its action, and every action is in "Action needed".
