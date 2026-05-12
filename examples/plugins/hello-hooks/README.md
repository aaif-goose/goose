# hello-hooks

A tiny [Open Plugins](https://open-plugins.com) plugin that demonstrates goose's
hook system. It registers four event handlers — `SessionStart`,
`UserPromptSubmit`, `PreToolUse`, and `PostToolUse` (matched on
`developer__shell|developer__text_editor`) — that each shell out to
`scripts/announce.sh` to print a noticeable line to stderr and append the full
event payload to `last-event.log` next to the plugin.

## Layout

```
hello-hooks/
├── plugin.json
├── hooks/
│   └── hooks.json
└── scripts/
    └── announce.sh
```

## Try it

Goose discovers plugins under `~/.agents/plugins/<name>/` (user scope) and
`<project-root>/.agents/plugins/<name>/` (project scope) per the Open Plugins
[installation spec](https://open-plugins.com/plugin-builders/installation#recommended-storage-paths).

```bash
mkdir -p ~/.agents/plugins
cp -R examples/plugins/hello-hooks ~/.agents/plugins/hello-hooks
chmod +x ~/.agents/plugins/hello-hooks/scripts/announce.sh

# Then run goose normally; you should see lines like
# 🚀 [hello-hooks] SessionStart
# 💬 [hello-hooks] UserPromptSubmit
# ⚡ [hello-hooks] PreToolUse tool=developer__shell
# ✅ [hello-hooks] PostToolUse tool=developer__shell
goose session

# Inspect the full payloads goose passed to the hook:
tail ~/.agents/plugins/hello-hooks/last-event.log
```

To turn the plugin off, add it to `disabledPlugins` in
`~/.config/goose/settings.json`:

```json
{ "disabledPlugins": ["hello-hooks"] }
```
