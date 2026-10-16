import { useEffect, useRef } from 'react';
import { acpReadClientExtensionMain } from '../acp/clientExtensions';
import { useClientExtensions } from './ClientExtensionsContext';
import { activateNativePlugin, deactivateNativePlugin } from './nativePluginStore';
import { useHostActions } from './useHostActions';

export function NativePluginRuntime() {
  const { extensions } = useClientExtensions();
  const actions = useHostActions();

  const nativeExtensions = extensions.filter(
    (extension) => extension.enabled && extension.manifest.runtime === 'native'
  );
  const signature = JSON.stringify(
    nativeExtensions.map((extension) => [
      extension.id,
      extension.manifest.version,
      extension.manifest.main,
      extension.manifest.permissions ?? [],
    ])
  );
  const latest = useRef(nativeExtensions);

  useEffect(() => {
    latest.current = nativeExtensions;
  });

  useEffect(() => {
    let cancelled = false;
    const targets = latest.current;

    for (const extension of targets) {
      void (async () => {
        try {
          const code = await acpReadClientExtensionMain(extension.id);
          if (!cancelled) {
            await activateNativePlugin(extension, code, actions);
          }
        } catch (error) {
          console.warn(`[client-extensions] Failed to activate "${extension.id}":`, error);
        }
      })();
    }

    return () => {
      cancelled = true;
      for (const extension of targets) {
        deactivateNativePlugin(extension.id);
      }
    };
  }, [actions, signature]);

  return null;
}
