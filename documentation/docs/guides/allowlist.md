---
sidebar_position: 90
title: Desktop Extension Install Allowlist
sidebar_label: Extension Allowlist
---

goose Desktop can receive extension installation requests from `goose://` links. By default, Desktop asks you to confirm each request. You can use an allowlist to block requests whose commands do not match an approved command prefix.

:::warning Scope
The allowlist applies only to extension installation links opened in goose Desktop. It does not apply to the CLI, recipes, schedules, extensions in `config.yaml`, `--with-extension`, or extensions that are already installed.
:::

## How It Works

1. The allowlist is a YAML file that contains allowed extension command prefixes.
2. goose Desktop fetches it from the URL in the `GOOSE_ALLOWLIST` environment variable when it handles an extension installation link.
3. Desktop compares the requested command with the listed prefixes.
4. If the command does not start with a listed value, Desktop blocks the installation request.

## Configuration

### 1. Create and Deploy Allowlist

The allowlist must be a YAML file with the following structure:

```yaml
extensions:
  - id: extension-id-1
    command: command-name-1
  - id: extension-id-2
    command: command-name-2
  # ... more extensions
```

#### Example

In this example, Desktop accepts installation links whose commands start with one of these values:

```yaml
extensions:
  - id: slack
    command: uvx mcp_slack
  - id: github
    command: uvx mcp_github
  - id: jira
    command: uvx mcp_jira
```


After creating the allowlist, you must deploy it to a URL.


### 2. Set Environment Variable

Create an environment variable called `GOOSE_ALLOWLIST` and set the value to the URL of your YAML file:

```bash
export GOOSE_ALLOWLIST=https://example.com/goose-allowlist.yaml
```

You can also add this export to your shell configuration file (On a Mac, it's your `~/.bashrc` or `~/.zshrc` file). 

:::info
If this environment variable is not set, Desktop uses its normal extension installation confirmation.
:::


## Best Practices

Allowlist entries use prefix matching. Keep each prefix as specific as possible:

1. **Include expected arguments**: An entry such as `npx` allows every requested command that starts with `npx`. Include the package and arguments needed by the extension.
2. **Include full paths**: To accept a command only from a specific path, include the full path in the allowlist.
3. **Audit regularly**: Review your allowlist frequently to ensure it only contains the commands you intend to allow.
4. **Use HTTPS**: Use an HTTPS URL for your allowlist to prevent man-in-the-middle attacks.
5. **Restrict edit access**: Ensure that only authorized users can edit the allowlist.
6. **Validate entries**: Carefully review the allowlist to ensure only trusted commands are included.
7. **Monitor installations**: Watch for rejected commands during extension installation, which might indicate attempted abuse.


## Troubleshooting

If extensions are being rejected unexpectedly:

1. Check if the `GOOSE_ALLOWLIST` environment variable is set correctly.
2. Verify that the allowlist file is accessible from the server.
3. Ensure the allowlist file is properly formatted YAML.
4. Check [Desktop logs](/docs/guides/logs) for errors related to fetching or parsing the allowlist.
5. Verify that the command in the installation link starts with a value in the allowlist.
