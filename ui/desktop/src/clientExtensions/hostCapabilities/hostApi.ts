import type { HostPermission } from './permissions';
import { findHostCapability, findHostMethod } from './registry';
import type { HostActions, HostCallContext } from './types';

export type HostEventListener = (capability: string, event: string, payload: unknown) => void;

export interface HostApi {
  permissions: readonly HostPermission[];
  invoke: (capability: string, method: string, payload?: unknown) => Promise<unknown>;
  subscribe: (listener: HostEventListener) => () => void;
  reset: () => void;
  dispose: () => void;
}

export function createHostApi(
  extensionId: string,
  permissions: readonly HostPermission[] | undefined,
  actions: HostActions,
  allowedOrigins: readonly string[] = []
): HostApi {
  const granted = new Set<HostPermission>(permissions);
  const disposers = new Map<string, () => void>();
  const listeners = new Set<HostEventListener>();

  const runDisposer = (key: string) => {
    const dispose = disposers.get(key);
    disposers.delete(key);
    dispose?.();
  };

  const runAllDisposers = () => {
    for (const key of [...disposers.keys()]) {
      runDisposer(key);
    }
  };

  const contextFor = (capability: string): HostCallContext => ({
    extensionId,
    actions,
    allowedOrigins,
    emit: (event, payload) => {
      for (const listener of [...listeners]) {
        listener(capability, event, payload);
      }
    },
    setDisposer: (key, dispose) => {
      const scopedKey = `${capability}:${key}`;
      runDisposer(scopedKey);
      disposers.set(scopedKey, dispose);
    },
    clearDisposer: (key) => runDisposer(`${capability}:${key}`),
  });

  return {
    permissions: [...granted],

    async invoke(capability, method, payload) {
      if (!findHostCapability(capability)) {
        throw new Error(`Unknown host capability "${capability}"`);
      }

      const definition = findHostMethod(capability, method);
      if (!definition) {
        throw new Error(`Unknown method "${capability}.${method}"`);
      }

      if (!granted.has(definition.permission)) {
        throw new Error(
          `Plugin "${extensionId}" is not granted permission "${definition.permission}"`
        );
      }

      return definition.handle(contextFor(capability), payload);
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    reset: runAllDisposers,

    dispose() {
      runAllDisposers();
      listeners.clear();
    },
  };
}
