import type { ClientExtensionInfo } from '@aaif/goose-acp-client';
import { parseClientExtensionManifest, satisfiesGrcEngine } from '../client-extensions/manifest';
import type { DiscoveredClientExtension } from '../client-extensions/types';
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
