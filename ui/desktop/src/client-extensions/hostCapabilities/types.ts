import type { HostPermission } from './permissions';

export interface HostCallContext {
  extensionId: string;
  emit: (event: string, payload?: unknown) => void;
  setDisposer: (key: string, dispose: () => void) => void;
  clearDisposer: (key: string) => void;
}

export interface HostMethodDefinition {
  permission: HostPermission;
  handle: (context: HostCallContext, payload: unknown) => Promise<unknown> | unknown;
}

export interface HostCapabilityDefinition {
  id: string;
  description: string;
  methods: Record<string, HostMethodDefinition>;
}

export type HostCapabilityHostMessage =
  | {
      type: 'grc/host/permissions';
      permissions: HostPermission[];
    }
  | {
      type: 'grc/host/result';
      capability: string;
      method: string;
      id?: string;
      payload?: unknown;
    }
  | {
      type: 'grc/host/error';
      capability: string;
      method: string;
      id?: string;
      error: string;
    }
  | {
      type: 'grc/host/event';
      capability: string;
      event: string;
      payload?: unknown;
    };
