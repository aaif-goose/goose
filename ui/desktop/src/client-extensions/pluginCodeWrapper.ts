export const DEFINE_PLUGIN_NAME = 'defineGoosePlugin';

export function wrapPluginCode(code: string): string {
  return `(function (${DEFINE_PLUGIN_NAME}) {\n${code}\n})(window.${DEFINE_PLUGIN_NAME});`;
}
