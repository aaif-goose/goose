import type { HostCapabilityInvokeMessage } from '../messages';
import type { HostPermission } from './permissions';
import { findHostCapability, findHostMethod } from './registry';
import type { HostCallContext, HostCapabilityHostMessage } from './types';

export interface HostSession {
  notifyPermissions: () => void;
  handleInvoke: (message: HostCapabilityInvokeMessage) => Promise<void>;
  dispose: () => void;
}

export function createHostSession(
  extensionId: string,
  permissions: readonly HostPermission[] | undefined,
  postToExtension: (message: HostCapabilityHostMessage) => void
): HostSession {
  const granted = new Set<HostPermission>(permissions);
  const disposers = new Map<string, () => void>();
  let disposed = false;

  const post = (message: HostCapabilityHostMessage) => {
    if (!disposed) {
      postToExtension(message);
    }
  };

  const runDisposer = (key: string) => {
    const dispose = disposers.get(key);
    disposers.delete(key);
    dispose?.();
  };

  const contextFor = (capability: string): HostCallContext => ({
    extensionId,
    emit: (event, payload) => post({ type: 'grc/host/event', capability, event, payload }),
    setDisposer: (key, dispose) => {
      const scopedKey = `${capability}:${key}`;
      runDisposer(scopedKey);
      disposers.set(scopedKey, dispose);
    },
    clearDisposer: (key) => runDisposer(`${capability}:${key}`),
  });

  return {
    notifyPermissions() {
      post({ type: 'grc/host/permissions', permissions: [...granted] });
    },

    async handleInvoke(message) {
      const { capability, method, id, payload } = message;
      const fail = (error: string) =>
        post({ type: 'grc/host/error', capability, method, id, error });

      if (!findHostCapability(capability)) {
        fail(`Unknown host capability "${capability}"`);
        return;
      }

      const definition = findHostMethod(capability, method);
      if (!definition) {
        fail(`Unknown method "${capability}.${method}"`);
        return;
      }

      if (!granted.has(definition.permission)) {
        fail(`Plugin "${extensionId}" is not granted permission "${definition.permission}"`);
        return;
      }

      try {
        const result = await definition.handle(contextFor(capability), payload);
        post({ type: 'grc/host/result', capability, method, id, payload: result });
      } catch (error) {
        fail(error instanceof Error ? error.message : String(error));
      }
    },

    dispose() {
      disposed = true;
      for (const key of [...disposers.keys()]) {
        runDisposer(key);
      }
    },
  };
}
