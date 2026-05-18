import type { FixedExtensionEntry } from '../../ConfigContext';
import type { ExtensionConfig } from '../../../api/types.gen';
import { parse as parseShellQuote } from 'shell-quote';

// Default extension timeout in seconds
// TODO: keep in sync with rust better

export const DEFAULT_EXTENSION_TIMEOUT = 300;

/**
 * Converts an extension name to a key format
 * TODO: need to keep this in sync better with `name_to_key` on the rust side
 */
export function nameToKey(name: string): string {
  return name
    .split('')
    .filter((char) => !char.match(/\s/))
    .join('')
    .toLowerCase();
}

export interface ExtensionFormData {
  name: string;
  description: string;
  type: 'stdio' | 'sse' | 'streamable_http' | 'builtin';
  cmd?: string;
  endpoint?: string;
  enabled: boolean;
  timeout?: number;
  envVars: {
    key: string;
    value: string;
    isEdited?: boolean;
  }[];
  headers: {
    key: string;
    value: string;
    isEdited?: boolean;
  }[];
  installation_notes?: string;
}

export function getDefaultFormData(): ExtensionFormData {
  return {
    name: '',
    description: '',
    type: 'stdio',
    cmd: '',
    endpoint: '',
    enabled: true,
    timeout: 300,
    envVars: [],
    headers: [],
  };
}

export function extensionToFormData(extension: FixedExtensionEntry): ExtensionFormData {
  // Type guard: Check if 'envs' property exists for this variant
  const hasEnvs = extension.type === 'streamable_http' || extension.type === 'stdio';

  // Handle both envs (legacy) and env_keys (new secrets)
  let envVars = [];

  // Add legacy envs with their values
  if (hasEnvs && extension.envs) {
    envVars.push(
      ...Object.entries(extension.envs).map(([key, value]) => ({
        key,
        value: value as string,
        isEdited: true, // We want to submit legacy values as secrets to migrate forward
      }))
    );
  }

  // Add env_keys with placeholder values
  if (hasEnvs && extension.env_keys) {
    envVars.push(
      ...extension.env_keys.map((key) => ({
        key,
        value: '••••••••', // Placeholder for secret values
        isEdited: false, // Mark as not edited initially
      }))
    );
  }

  // Handle headers for streamable_http
  let headers = [];
  if (extension.type === 'streamable_http' && 'headers' in extension && extension.headers) {
    headers.push(
      ...Object.entries(extension.headers).map(([key, value]) => ({
        key,
        value: value as string,
        isEdited: false, // Mark as not edited initially
      }))
    );
  }

  return {
    name: extension.name || '',
    description: extension.description || '',
    type:
      extension.type === 'frontend' ||
      extension.type === 'inline_python' ||
      extension.type === 'platform'
        ? 'stdio'
        : extension.type,
    cmd: extension.type === 'stdio' ? combineCmdAndArgs(extension.cmd, extension.args) : undefined,
    endpoint:
      extension.type === 'streamable_http' || extension.type === 'sse'
        ? (extension.uri ?? undefined)
        : undefined,
    enabled: extension.enabled,
    timeout: 'timeout' in extension ? (extension.timeout ?? undefined) : undefined,
    envVars,
    headers,
    installation_notes: (extension as Record<string, unknown>)['installation_notes'] as
      | string
      | undefined,
  };
}

export function createExtensionConfig(formData: ExtensionFormData): ExtensionConfig {
  // Extract just the keys from env vars
  const env_keys = formData.envVars.map(({ key }) => key).filter((key) => key.length > 0);

  if (formData.type === 'stdio') {
    // we put the cmd + args all in the form cmd field but need to split out into cmd + args
    const { cmd, args } = splitCmdAndArgs(formData.cmd || '');

    return {
      type: 'stdio',
      name: formData.name,
      description: formData.description,
      cmd: cmd,
      args: args,
      timeout: formData.timeout,
      ...(env_keys.length > 0 ? { env_keys } : {}),
    };
  } else if (formData.type === 'streamable_http') {
    // Extract headers
    const headers = formData.headers
      .filter(({ key, value }) => key.length > 0 && value.length > 0)
      .reduce(
        (acc, header) => {
          acc[header.key] = header.value;
          return acc;
        },
        {} as Record<string, string>
      );

    return {
      type: 'streamable_http',
      name: formData.name,
      description: formData.description,
      timeout: formData.timeout,
      uri: formData.endpoint || '',
      ...(env_keys.length > 0 ? { env_keys } : {}),
      headers,
    };
  } else {
    // For other types
    return {
      type: formData.type,
      name: formData.name,
      description: formData.description,
      timeout: formData.timeout,
    };
  }
}

export function splitCmdAndArgs(str: string): { cmd: string; args: string[] } {
  const trimmed = str.trim();
  if (!trimmed) {
    return { cmd: '', args: [] };
  }

  const parsed = parseShellQuote(trimmed);
  const words = parsed.filter((item): item is string => typeof item === 'string').map(String);

  const cmd = words[0] || '';
  const args = words.slice(1);

  return {
    cmd,
    args,
  };
}

export function combineCmdAndArgs(cmd: string, args: string[]): string {
  return [cmd, ...args]
    .map((a) => {
      if (!a.includes(' ')) return a;
      if (a.includes('"')) return `'${a}'`;
      return `"${a}"`;
    })
    .join(' ');
}

export function extractCommand(link: string): string {
  const url = new URL(link);
  const cmd = url.searchParams.get('cmd') || 'Unknown Command';
  const args = url.searchParams.getAll('arg').map(decodeURIComponent);

  // Combine the command and its arguments into a reviewable format
  return `${cmd} ${args.join(' ')}`.trim();
}

export function extractExtensionName(link: string): string {
  const url = new URL(link);
  const name = url.searchParams.get('name');
  return name ? decodeURIComponent(name) : 'Unknown Extension';
}
