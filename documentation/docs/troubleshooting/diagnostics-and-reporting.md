---
title: Diagnostics and Reporting
sidebar_label: Diagnostics and Reporting
description: Use built-in diagnostics, report bugs, and request new features with goose's integrated support tools.
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft, Bug, MessageSquarePlus, Download } from 'lucide-react';

goose provides several built-in features to help you get support, report issues, and request new functionality. This guide covers the diagnostics system, bug reporting, and feature request tools.

| Feature | Purpose | Location | Output |
|---------|---------|----------|---------|
| **Diagnostics** | Generate troubleshooting data | Chat input toolbar | ZIP file with system info, logs, and session data |
| **Report a Bug** | Submit bug reports | Chat input toolbar OR Settings → App → Help & feedback | Opens GitHub issue template |
| **Request a Feature** | Suggest new features | Settings → App → Help & feedback | Opens GitHub issue template |

## Diagnostics System

The diagnostics feature creates a comprehensive troubleshooting bundle that includes system information, session data, configuration files, and recent logs. This is invaluable for debugging issues or getting technical support.

### Generating Diagnostics

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    1. In an active chat session, look for the <Bug className="inline" size={16} /> icon in the bottom toolbar
    2. Click the diagnostics button
    3. Review the information in the modal about what data will be collected
    4. Click `Download` to generate and save the diagnostics bundle
    5. The ZIP file will be saved as `diagnostics_{session_id}.zip`

    :::tip
    The diagnostics button is only available when you have an active session, as it needs a session ID to generate the bundle.
    :::
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Use the session diagnostics command to generate a troubleshooting bundle. For complete details and all available options, see the [CLI Commands guide](/docs/guides/goose-cli-commands#session-diagnostics-options).

    ```sh
    # Generate diagnostics for a specific session
    goose session diagnostics --session-id <session_id>

    # Interactive selection (prompts you to choose a session)
    goose session diagnostics

    # Save to a custom location
    goose session diagnostics --session-id <session_id> --output /path/to/diagnostics.zip
    ```

    To find your session ID, first list available sessions:

    ```sh
    goose session list
    ```

    Example output:
    ```
    Available sessions:
    abc123def - My coding session - 2024-01-15 14:30:22
    xyz789ghi - Documentation work - 2024-01-15 10:15:45
    ```
  </TabItem>
</Tabs>

### Using Diagnostics Data

The diagnostics ZIP file contains several folders:

```
diagnostics_abc123def.zip
├── logs/
│   ├── goose-2024-01-15.jsonl
│   ├── goose-2024-01-14.jsonl
│   └── ...
├── session.json          # Your session messages
├── config.yaml          # Configuration files (if they exist)
└── system.txt           # System information
```

**When to generate diagnostics:**
- Experiencing crashes or unexpected behavior
- Getting error messages you don't understand
- Performance issues or slow responses
- Before reporting bugs to include technical details

**What's included in diagnostics:**
- **System Information**: App version, operating system, architecture, and timestamp
- **Session Data**: Your current conversation messages and history
- **Configuration Files**: Your [configuration files](/docs/guides/config-files) (if they exist)
- **Log Files**: Recent application logs for debugging

:::warning Privacy Notice
Diagnostics bundles contain your session messages and system information. If your session includes sensitive data (API keys, personal information, proprietary code), review the contents before sharing publicly.
:::

## Bug Reports

The bug report feature opens a structured GitHub issue template to help you provide all necessary information for effective bug reporting.

### Creating Bug Reports

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    1. In an active chat session, look for the <Bug className="inline" size={16} /> icon in the bottom toolbar
    2. Click the diagnostics button
    3. Click `File Bug on GitHub`
    4. This opens GitHub in your browser with a pre-filled bug report template
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    For CLI users, navigate directly to the GitHub repository:

    ```
    https://github.com/aaif-goose/goose/issues/new?template=bug_report.md
    ```
  </TabItem>
</Tabs>

## Feature Requests

The feature request system helps you suggest improvements and new functionality for goose.

### Submitting Feature Requests

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
    2. Click `Settings` in the sidebar
    3. Click the `App` tab
    4. Scroll down to the `Help & feedback` section
    5. Click `Request a Feature`
    6. This opens GitHub in your browser with a feature request template
  </TabItem>
  <TabItem value="cli" label="goose CLI">
    Navigate directly to the GitHub repository:

    ```
    https://github.com/aaif-goose/goose/issues/new?template=feature_request.md
    ```
  </TabItem>
</Tabs>

## Error Recovery with "Ask goose"

When certain types of error occur in goose Desktop (such as failures to activate extensions), you'll see an `Ask goose` button in the error notification. This feature lets you quickly troubleshoot the issue with goose's help:

1. When the error occurs, an `Ask goose` button appears in the error notification
2. Click the button to send the error details to goose in a chat prompt
3. goose provides diagnostic suggestions and potential solutions

## Additional Debugging

For issues not resolved by diagnostics:

- **[Session and System Logs](/docs/guides/logs)**: View detailed logs for debugging individual sessions
- **[Telemetry Export](/docs/guides/environment-variables#observability)**: Configure telemetry for performance analysis and production monitoring

