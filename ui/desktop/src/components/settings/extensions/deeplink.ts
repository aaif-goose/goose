import type { ExtensionConfig } from '../../../api';
import { toastService } from '../../../toasts';
import { DEFAULT_EXTENSION_TIMEOUT } from './utils';

/**
 * Build an extension config for stdio from the deeplink URL
 */
function getStdioConfig(
  cmd: string,
  parsedUrl: URL,
  name: string,
  description: string,
  timeout: number
) {
  // Validate that the command is one of the allowed commands
  const allowedCommands = [
    'cu',
    'docker',
    'jbang',
    'npx',
    'uvx',
    'goosed',
    'npx.cmd',
    'i-ching-mcp-server',
  ];
  if (!allowedCommands.includes(cmd)) {
    toastService.handleError(
      'Invalid Command',
      `Failed to install extension: Invalid command: ${cmd}. Only ${allowedCommands.join(', ')} are allowed.`,
      { shouldThrow: true }
    );
  }

  // Check for security risk with npx -c command
  const args = parsedUrl.searchParams.getAll('arg');
  if (cmd === 'npx' && args.includes('-c')) {
    toastService.handleError(
      'Security Risk',
      'Failed to install extension: npx with -c argument can lead to code injection',
      { shouldThrow: true }
    );
  }

  const envList = parsedUrl.searchParams.getAll('env');

  const config: ExtensionConfig = {
    name: name,
    type: 'stdio',
    cmd: cmd,
    description,
    args: args,
    envs:
      envList.length > 0
        ? Object.fromEntries(
            envList.map((env) => {
              const [key] = env.split('=');
              return [key, '']; // Initialize with empty string as value
            })
          )
        : undefined,
    timeout: timeout,
  };

  return config;
}

/**
 * Build an extension config for Streamable HTTP from the deeplink URL
 */
function getStreamableHttpConfig(
  remoteUrl: string,
  name: string,
  description: string,
  timeout: number,
  headers?: { [key: string]: string },
  envs?: { [key: string]: string }
) {
  const config: ExtensionConfig = {
    name,
    type: 'streamable_http',
    uri: remoteUrl,
    description,
    timeout: timeout,
    headers: headers,
    envs: envs,
  };

  return config;
}

/**
 * Handles adding an extension from a deeplink URL
 */
export async function addExtensionFromDeepLink(
  url: string,
  addExtensionFn: (
    name: string,
    extensionConfig: ExtensionConfig,
    enabled: boolean
  ) => Promise<void>,
  setView: (
    view: string,
    options: { showEnvVars: boolean; deepLinkConfig?: ExtensionConfig }
  ) => void
) {
  const parsedUrl = new URL(url);

  if (parsedUrl.protocol !== 'goose:') {
    toastService.handleError(
      'Invalid Protocol',
      'Failed to install extension: Invalid protocol: URL must use the goose:// scheme',
      { shouldThrow: true }
    );
  }

  // Check that all required fields are present and not empty
  const requiredFields = ['name'];

  for (const field of requiredFields) {
    const value = parsedUrl.searchParams.get(field);
    if (!value || value.trim() === '') {
      toastService.handleError(
        'Missing Field',
        `Failed to install extension: The link is missing required field '${field}'`,
        { shouldThrow: true }
      );
    }
  }

  const name = parsedUrl.searchParams.get('name')!;
  const parsedTimeout = parsedUrl.searchParams.get('timeout');
  const timeout = parsedTimeout ? parseInt(parsedTimeout, 10) : DEFAULT_EXTENSION_TIMEOUT;
  const description = parsedUrl.searchParams.get('description');
  const installation_notes = parsedUrl.searchParams.get('installation_notes');

  const cmd = parsedUrl.searchParams.get('cmd');
  const remoteUrl = parsedUrl.searchParams.get('url');

  const headerParams = parsedUrl.searchParams.getAll('header');
  const headers =
    headerParams.length > 0
      ? Object.fromEntries(
          headerParams.map((header) => {
            const [key, value] = header.split('=');
            return [key, decodeURIComponent(value || '')];
          })
        )
      : undefined;

  // Parse env vars for remote extensions (same logic as stdio)
  const envList = parsedUrl.searchParams.getAll('env');
  const envs =
    envList.length > 0
      ? Object.fromEntries(
          envList.map((env) => {
            const [key] = env.split('=');
            return [key, ''];
          })
        )
      : undefined;

  const baseConfig = remoteUrl
    ? getStreamableHttpConfig(remoteUrl, name, description || '', timeout, headers, envs)
    : getStdioConfig(cmd!, parsedUrl, name, description || '', timeout);

  const config = {
    ...baseConfig,
    ...(installation_notes ? { installation_notes } : {}),
  };

  // Check if extension requires env vars or headers and go to settings if so
  const hasEnvVars = config.envs && Object.keys(config.envs).length > 0;
  const hasHeaders =
    config.type === 'streamable_http' && config.headers && Object.keys(config.headers).length > 0;

  if (hasEnvVars || hasHeaders) {
    setView('extensions', { deepLinkConfig: config, showEnvVars: true });
    return;
  }

  // Note: deeplink activation doesn't have access to sessionId
  // The extension will be added to config but not activated in the current session
  // It will be activated when the next session starts
  await addExtensionFn(config.name, config, true);

  // Show success toast and navigate to extensions page
  toastService.success({
    title: 'Extension Installed',
    msg: `${config.name} extension has been installed successfully. Start a new chat session to use it.`,
  });

  // Navigate to extensions page to show the newly installed extension
  setView('extensions', { deepLinkConfig: config, showEnvVars: false });
}
