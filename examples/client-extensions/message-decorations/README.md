# message-decorations

Demo GRC client extension for Phase 3 message slots.

## Contributions

- **contentSuffix** (`image-badge`) — shows a badge when a message includes an image (`when: message.hasImage`).
- **customRender** (`json-preview`) — renders a formatted JSON preview for assistant messages containing a ` ```json ` code block.

## Try it

When running goose Desktop from source, this extension is auto-discovered from `examples/client-extensions/`.

Ask goose to reply with a JSON code block, or send a message with an image attachment, to see the slots in action.
