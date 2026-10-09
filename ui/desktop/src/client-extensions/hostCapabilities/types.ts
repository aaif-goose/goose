import type { HostPermission } from './permissions';

export interface StartChatOptions {
  prompt?: string;
  recipeId?: string;
  workingDir?: string;
}

export interface HostActions {
  startChat: (options: StartChatOptions) => Promise<string>;
  createSession: (workingDir?: string) => Promise<string>;
  openSession: (sessionId: string) => void;
  openPage: (extensionId: string, viewId: string) => void;
}

export interface HostCallContext {
  extensionId: string;
  actions: HostActions;
  allowedOrigins: readonly string[];
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
