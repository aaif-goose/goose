import type { ClientExtensionInfo } from '@aaif/goose-acp-client';
import { parseClientExtensionManifest, satisfiesGrcEngine } from '../client-extensions/manifest';
import type {
  ClientExtensionNetFetchResult,
  DiscoveredClientExtension,
} from '../client-extensions/types';
import { getAcpClient } from './acpConnection';

export interface ClientExtensionsListing {
  installDir: string;
  extensions: DiscoveredClientExtension[];
}

interface RawListing {
  installDir: string;
  extensions: ClientExtensionInfo[];
}

function toDiscovered(info: ClientExtensionInfo): DiscoveredClientExtension | null {
  const manifest = parseClientExtensionManifest(info.manifest);
  if (!manifest) {
    console.warn(`[client-extensions] Invalid manifest for "${info.id}"`);
    return null;
  }
  if (!satisfiesGrcEngine(manifest, window.electron.getVersion())) {
    console.warn(
      `[client-extensions] Skipping "${info.id}": requires GRC ${manifest.engines?.grc}`
    );
    return null;
  }
  return {
    id: info.id,
    manifest,
    source: info.source,
    enabled: info.enabled,
  };
}

function toListing(raw: RawListing): ClientExtensionsListing {
  return {
    installDir: raw.installDir,
    extensions: raw.extensions
      .map(toDiscovered)
      .filter((extension): extension is DiscoveredClientExtension => extension !== null),
  };
}

export async function acpListClientExtensions(): Promise<ClientExtensionsListing> {
  const client = await getAcpClient();
  return toListing(await client.goose.clientExtensionsList_unstable({}));
}

export async function acpInstallClientExtension(
  sourcePath: string
): Promise<ClientExtensionsListing> {
  const client = await getAcpClient();
  const response = await client.goose.clientExtensionsInstall_unstable({ sourcePath });
  const listing = toListing(response);
  if (listing.extensions.some((extension) => extension.id === response.installedId)) {
    return listing;
  }

  await client.goose.clientExtensionsUninstall_unstable({ id: response.installedId });
  throw new Error(`Plugin "${response.installedId}" is not compatible with this version of Goose`);
}

export async function acpSetClientExtensionEnabled(
  id: string,
  enabled: boolean
): Promise<ClientExtensionsListing> {
  const client = await getAcpClient();
  return toListing(await client.goose.clientExtensionsSetEnabled_unstable({ id, enabled }));
}

export async function acpUninstallClientExtension(id: string): Promise<ClientExtensionsListing> {
  const client = await getAcpClient();
  return toListing(await client.goose.clientExtensionsUninstall_unstable({ id }));
}

export async function acpReadClientExtensionMain(id: string): Promise<string> {
  const client = await getAcpClient();
  const { html } = await client.goose.clientExtensionsReadMain_unstable({ id });
  return html;
}

export async function acpGetClientExtensionStorage(
  extensionId: string,
  key: string
): Promise<unknown> {
  const client = await getAcpClient();
  const { value } = await client.goose.clientExtensionsStorageGet_unstable({ extensionId, key });
  return value ?? null;
}

export async function acpSetClientExtensionStorage(
  extensionId: string,
  key: string,
  value: unknown
): Promise<void> {
  const client = await getAcpClient();
  await client.goose.clientExtensionsStorageSet_unstable({ extensionId, key, value });
}

export async function acpDeleteClientExtensionStorage(
  extensionId: string,
  key: string
): Promise<boolean> {
  const client = await getAcpClient();
  const { existed } = await client.goose.clientExtensionsStorageDelete_unstable({
    extensionId,
    key,
  });
  return existed;
}

export async function acpListClientExtensionStorageKeys(extensionId: string): Promise<string[]> {
  const client = await getAcpClient();
  const { keys } = await client.goose.clientExtensionsStorageKeys_unstable({ extensionId });
  return keys;
}

export async function acpFetchClientExtensionNet(
  extensionId: string,
  url: string,
  method?: string,
  headers?: Record<string, string>,
  body?: string
): Promise<ClientExtensionNetFetchResult> {
  const client = await getAcpClient();
  const response = await client.goose.clientExtensionsNetFetch_unstable({
    extensionId,
    url,
    method,
    headers,
    body,
  });
  return { ...response, headers: response.headers ?? {} };
}
