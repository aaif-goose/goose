import { useEffect, useRef } from 'react';
import {
  createHostSession,
  type HostCapabilityHostMessage,
  type HostSession,
} from './hostCapabilities';
import { useClientExtensions } from './ClientExtensionsContext';
import { useHostActions } from './useHostActions';
import type { DiscoveredClientExtension } from './types';

export function useExtensionHostSession(
  extension: DiscoveredClientExtension | undefined,
  postToExtension: (message: HostCapabilityHostMessage) => void
) {
  const hostActions = useHostActions();
  const { registryVersion } = useClientExtensions();
  const sessionRef = useRef<HostSession | null>(null);

  useEffect(() => {
    if (!extension) {
      sessionRef.current = null;
      return;
    }

    const session = createHostSession(
      extension.id,
      extension.manifest.permissions,
      postToExtension,
      hostActions,
      extension.manifest.network
    );
    sessionRef.current = session;
    return () => {
      session.dispose();
      sessionRef.current = null;
    };
  }, [extension, hostActions, postToExtension, registryVersion]);

  return sessionRef;
}
