import React, { type ComponentType } from 'react';
import type { HostApi } from './hostCapabilities';
import { COMMON_HOST_POWERS } from './hostCapabilities/powers';
import { DEFINE_PLUGIN_NAME, wrapPluginCode } from './pluginCodeWrapper';

export type PluginComponent = ComponentType;

export type HostPowerApi = Record<string, (payload?: unknown) => Promise<unknown>>;

export interface PluginApi {
  extensionId: string;
  react: typeof React;
  host: Record<string, HostPowerApi>;
  on: (capability: string, event: string, listener: (payload: unknown) => void) => () => void;
  pages: {
    register: (viewId: string, component: PluginComponent) => void;
  };
  commands: {
    register: (
      id: string,
      description: string,
      run: (args: unknown) => Promise<unknown> | unknown
    ) => void;
  };
}

export interface PluginDefinition {
  activate: (api: PluginApi) => void | Promise<void>;
  deactivate?: () => void;
}

export function loadPluginDefinition(code: string): PluginDefinition {
  const captured: { definition: PluginDefinition | null } = { definition: null };
  Reflect.set(window, DEFINE_PLUGIN_NAME, (definition: PluginDefinition) => {
    captured.definition = definition;
  });

  const script = document.createElement('script');
  script.textContent = wrapPluginCode(code);
  try {
    document.head.appendChild(script);
  } finally {
    script.remove();
    Reflect.deleteProperty(window, DEFINE_PLUGIN_NAME);
  }

  if (typeof captured.definition?.activate !== 'function') {
    throw new Error(`Plugin code must call ${DEFINE_PLUGIN_NAME}({ activate })`);
  }
  return captured.definition;
}

export function createPluginApi(
  extensionId: string,
  hostApi: HostApi,
  registerPage: PluginApi['pages']['register'],
  registerCommand: PluginApi['commands']['register']
): PluginApi {
  const host = Object.fromEntries(
    COMMON_HOST_POWERS.map((power) => [
      power.id,
      Object.fromEntries(
        Object.keys(power.methods).map((method) => [
          method,
          (payload?: unknown) => hostApi.invoke(power.id, method, payload),
        ])
      ),
    ])
  );

  return {
    extensionId,
    react: React,
    host,
    on: (capability, event, listener) =>
      hostApi.subscribe((emittedCapability, emittedEvent, payload) => {
        if (emittedCapability === capability && emittedEvent === event) {
          listener(payload);
        }
      }),
    pages: { register: registerPage },
    commands: { register: registerCommand },
  };
}
