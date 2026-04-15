---
sidebar_position: 1
title: Prompt Injection Detection
sidebar_label: Prompt Injection Detection
description: Protect your workflow by detecting potentially harmful commands before they run.
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PanelLeft, Settings } from 'lucide-react';

Prompt injection happens when malicious instructions are hidden inside executable content. In the world of AI, prompt injection can be used to nudge AI agents (like goose) to run unsafe commands that compromise your environment or data.

You can help protect your goose workflows by enabling prompt injection detection. This feature uses pattern matching to detect common attack techniques, including:
- Attempts to delete system files or directories
- Commands that download and execute remote scripts
- Attempts to access or exfiltrate sensitive data like SSH keys
- System modifications that could compromise security

In addition, you can optionally enable [ML-based scanning](#enhanced-detection-with-machine-learning) using a specified model.

:::important
These checks provide a safeguard, not a guarantee. They detect known patterns but cannot catch all possible threats, especially novel or sophisticated attacks.
:::

## How Detection Works

When enabled, goose uses a multi-layered approach to detect threats before they run:

1. **Tool call is intercepted and analyzed** - When goose prepares to execute a tool, the security system extracts the tool parameter text and checks it against [threat patterns](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/security/patterns.rs). If ML-based detection is enabled, it also uses machine learning to analyze the semantic content of the tool call and recent conversation messages to better understand context and reduce false positives.
2. **Risk is assessed** - Detected threats are assigned confidence scores
3. **Execution pauses** - Threats that exceed your configured threshold need your decision
4. **Security alert appears** - The alert displays the confidence level, a description of the finding, and a unique finding ID. For example:
   ```
   🔒 Security Alert: This tool call has been flagged as potentially dangerous.
   
   Confidence: 95%
   Explanation: Detected 1 security threat: Recursive file deletion with rm -rf
   Finding ID: SEC-abc123...
   
   [Allow Once] [Deny]
   ```
5. **You choose** whether to proceed or cancel after reviewing the alert details. Note that:
   - Each decision is logged with its finding ID in the [goose system logs](/docs/guides/logs#system-logs)
   - Allowed commands still run with your full permissions

**Responding to Alerts:**

- Read the explanation to understand what triggered the detection
- Consider your context&mdash;does this match what you're trying to do?
- Try rephrasing your request more specifically
- Check the source and be extra cautious with prompts from unknown sources

When in doubt, deny. 

## Enabling Detection

<Tabs groupId="interface">
  <TabItem value="ui" label="goose Desktop" default>
    
    1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
    2. Click `Settings` on the sidebar
    3. Click the `Chat` tab
    4. Toggle `Enable Prompt Injection Detection` to the on setting
    5. Optionally adjust the `Detection Threshold` to [configure the sensitivity](#configuring-detection-threshold)
    6. Optionally enable ML-based detection:
       1. Toggle `Enable ML-based Detection` to the on setting
       2. Configure your inference endpoint:
          - `Endpoint URL`: URL to the classification service (e.g., Hugging Face)
          - `API Token`: Authentication token if required by your service

  </TabItem>
  <TabItem value="config" label="goose config file">

    Add security prompt settings to your [`config.yaml`](/docs/guides/config-files):

    ```yaml
    SECURITY_PROMPT_ENABLED: true
    SECURITY_PROMPT_THRESHOLD: 0.8  # Optional, default is 0.8

    # Optional: Enable ML-based detection (Hugging Face example)
    SECURITY_PROMPT_CLASSIFIER_ENABLED: true
    SECURITY_PROMPT_CLASSIFIER_ENDPOINT: "https://router.huggingface.co/hf-inference/models/protectai/deberta-v3-base-prompt-injection-v2"
    SECURITY_PROMPT_CLASSIFIER_TOKEN: "YOUR_HUGGING_FACE_TOKEN"
    ```

  </TabItem>
</Tabs>

:::info Other Security Features
Beyond prompt injection detection, goose automatically:
- Warns you before running new or updated recipes
- Warns you when importing recipes that contain invisible Unicode Tag Block characters
- [Checks for known malware](/docs/troubleshooting/known-issues#malicious-package-detected) when installing extensions for locally-run MCP servers
:::

### Configuring Detection Threshold

The threshold (0.01-1.0) controls how strict detection is:

| Threshold | Sensitivity | Use When |
|-----------|------------|----------|
| **0.01-0.50** | Very lenient | You're experienced and understand the risks |
| **0.50-0.70** | Balanced | General development work (good default) |
| **0.70-0.90** | Strict | Working with sensitive data or systems |
| **0.90-1.00** | Maximum | High-security environments |

When the injection prompt detection feature is enabled, the default threshold is 0.8 (recommended for most users).

Lower thresholds mean fewer alerts but might miss threats. Higher thresholds catch more potential issues but may flag legitimate operations. You can control this sensitivity/convenience tradeoff based on your needs.

## Enhanced Detection with Machine Learning

By default, prompt injection detection uses pattern matching, but you can optionally enable ML-based detection for improved accuracy and fewer false positives.

ML-based detection:
- Analyzes the semantic content of tool calls and recent messages
- Detects sophisticated attacks that patterns might miss
- Reduces false positives by understanding conversation context
- Requires providing a classification endpoint URL and API token (if required)

:::warning Privacy Consideration
When ML-based detection is enabled, tool call content and recent messages are sent to the configured endpoint for analysis.
:::

#### Self-Hosting ML Detection Endpoints
If you want to run your own classification endpoint, see the [Classification API Specification](/docs/guides/security/classification-api-spec) for implementation details. The API follows the Hugging Face Inference API format.

## See Also

- [goose Permission Modes](/docs/guides/goose-permissions) - Control goose's autonomy level
- [Managing Tool Permissions](/docs/guides/managing-tools/tool-permissions) - Fine-grained tool control
