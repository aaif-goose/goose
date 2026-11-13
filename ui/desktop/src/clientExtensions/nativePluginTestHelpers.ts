import { DEFINE_PLUGIN_NAME, wrapPluginCode } from './pluginCodeWrapper';
import type { PluginDefinition } from './nativePlugin';

export function evalPluginCode(code: string): PluginDefinition {
  const captured: { definition: PluginDefinition | null } = { definition: null };
  Reflect.set(window, DEFINE_PLUGIN_NAME, (definition: PluginDefinition) => {
    captured.definition = definition;
  });
  try {
    new Function(wrapPluginCode(code))();
  } finally {
    Reflect.deleteProperty(window, DEFINE_PLUGIN_NAME);
  }

  if (typeof captured.definition?.activate !== 'function') {
    throw new Error(`Plugin code must call ${DEFINE_PLUGIN_NAME}({ activate })`);
  }
  return captured.definition;
}
