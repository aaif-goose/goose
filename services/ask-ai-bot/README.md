# ask-ai-bot

Discord support bot for goose. Replies normally stay under 120 words. It searches
relevant documentation sections, source code, recent GitHub comments, and releases.

Run `bun run dev` here with `DISCORD_TOKEN`, `QUESTION_CHANNEL_ID`, and
`ANTHROPIC_API_KEY` in `.env`. `GITHUB_TOKEN` is optional.

Compact thread notes live in `.data/threads/` (`THREAD_STATE_PATH` overrides it).
For Docker, mount a persistent volume at `/app/data` to retain notes across
container replacements, e.g. `-v ask-ai-data:/app/data`.
