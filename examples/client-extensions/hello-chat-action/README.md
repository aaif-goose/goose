# hello-chat-action

Example GRC client extension for the chat action slot.

## Try it

Copy into the user extensions directory:

```bash
mkdir -p ~/.agents/client-extensions
cp -R examples/client-extensions/hello-chat-action ~/.agents/client-extensions/
```

Restart goose Desktop. You should see **Hello** and **Fill** buttons in the chat input bar (Fill only appears when a session is active).

## Manifest

See `client-extension.json`. Extensions are discovered from:

- `~/.agents/client-extensions/<id>/`
- `examples/client-extensions/` when running the desktop app from source (dev only)
