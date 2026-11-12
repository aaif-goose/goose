# ask-ai-bot

Discord support bot for goose. Replies normally stay under 120 words. It searches
relevant documentation sections, source code, recent GitHub comments, and releases.

Run `bun run dev` here with `DISCORD_TOKEN`, `QUESTION_CHANNEL_ID`, and
`ANTHROPIC_API_KEY` in `.env`. `GITHUB_TOKEN` is optional.

Failures log the thread, model, phase, finish reason, token usage, and error causes.
Set `CONSOLA_LEVEL=4` in `.env` for step/tool timing and generation diagnostics;
`CONSOLA_LEVEL=5` also includes error stack traces. Default is `3`.
Logs omit prompts, attachments, tool inputs/outputs, and raw provider bodies.

Compact thread notes live in `.data/threads/` (`THREAD_STATE_PATH` overrides it).
For Docker, mount a persistent volume at `/app/data` to retain notes across
container replacements, e.g. `-v ask-ai-data:/app/data`.
