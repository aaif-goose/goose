import type { HostCapabilityInvokeMessage } from '../messages';
import { createHostApi } from './hostApi';
import type { HostPermission } from './permissions';
import type { HostActions, HostCapabilityHostMessage } from './types';

export interface HostSession {
  notifyPermissions: () => void;
  handleInvoke: (message: HostCapabilityInvokeMessage) => Promise<void>;
  reset: () => void;
  dispose: () => void;
}

export function createHostSession(
  extensionId: string,
  permissions: readonly HostPermission[] | undefined,
  postToExtension: (message: HostCapabilityHostMessage) => void,
  actions: HostActions,
  allowedOrigins: readonly string[] = []
): HostSession {
  const api = createHostApi(extensionId, permissions, actions, allowedOrigins);
  let disposed = false;

  const post = (message: HostCapabilityHostMessage) => {
    if (!disposed) {
      postToExtension(message);
    }
  };

  api.subscribe((capability, event, payload) =>
    post({ type: 'grc/host/event', capability, event, payload })
  );

  return {
    notifyPermissions() {
      post({ type: 'grc/host/permissions', permissions: [...api.permissions] });
    },

    async handleInvoke({ capability, method, id, payload }) {
      try {
        const result = await api.invoke(capability, method, payload);
        post({ type: 'grc/host/result', capability, method, id, payload: result });
      } catch (error) {
        post({
          type: 'grc/host/error',
          capability,
          method,
          id,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    },

    reset: api.reset,

    dispose() {
      disposed = true;
      api.dispose();
    },
  };
}
