import { createHostApi, type HostActions, type HostApi } from './hostCapabilities';
import {
  createPluginApi,
  loadPluginDefinition,
  type PluginComponent,
  type PluginDefinition,
} from './nativePlugin';
import { registerPluginCommand, unregisterPluginCommands } from './pluginCommandRegistry';
import type { DiscoveredClientExtension } from './types';

interface ActivePlugin {
  definition: PluginDefinition;
  hostApi: HostApi;
  pages: Map<string, PluginComponent>;
}

const activePlugins = new Map<string, ActivePlugin>();
const failedPlugins = new Set<string>();
const listeners = new Set<() => void>();
let version = 0;

function notify(): void {
  version += 1;
  for (const listener of [...listeners]) {
    listener();
  }
}

export function subscribeNativePlugins(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getNativePluginsVersion(): number {
  return version;
}

export function getNativePluginPage(
  extensionId: string,
  viewId: string
): PluginComponent | undefined {
  return activePlugins.get(extensionId)?.pages.get(viewId);
}

export function hasNativePluginFailed(extensionId: string): boolean {
  return failedPlugins.has(extensionId);
}

export function deactivateNativePlugin(extensionId: string): void {
  failedPlugins.delete(extensionId);
  unregisterPluginCommands(extensionId);
  const plugin = activePlugins.get(extensionId);
  if (!plugin) {
    return;
  }

  activePlugins.delete(extensionId);
  try {
    plugin.definition.deactivate?.();
  } catch (error) {
    console.warn(`[client-extensions] Plugin "${extensionId}" failed to deactivate:`, error);
  }
  plugin.hostApi.dispose();
  notify();
}

export async function activateNativePlugin(
  extension: DiscoveredClientExtension,
  code: string,
  actions: HostActions
): Promise<void> {
  const extensionId = extension.id;
  deactivateNativePlugin(extensionId);

  const hostApi = createHostApi(
    extensionId,
    extension.manifest.permissions,
    actions,
    extension.manifest.network
  );
  let entry: ActivePlugin | undefined;

  try {
    const definition = loadPluginDefinition(code);
    const pages = new Map<string, PluginComponent>();
    entry = { definition, hostApi, pages };
    activePlugins.set(extensionId, entry);

    await definition.activate(
      createPluginApi(
        extensionId,
        hostApi,
        (viewId, component) => {
          if (activePlugins.get(extensionId) !== entry) {
            return;
          }
          pages.set(viewId, component);
          notify();
        },
        (id, description, run) => {
          if (activePlugins.get(extensionId) !== entry) {
            return;
          }
          registerPluginCommand(extensionId, id, description, run);
        }
      )
    );

    if (activePlugins.get(extensionId) !== entry) {
      hostApi.dispose();
    }
  } catch (error) {
    if (activePlugins.get(extensionId) === entry) {
      activePlugins.delete(extensionId);
      failedPlugins.add(extensionId);
      notify();
    }
    hostApi.dispose();
    throw error;
  }
}
