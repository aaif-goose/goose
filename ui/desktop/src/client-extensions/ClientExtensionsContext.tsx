import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useLocation } from 'react-router';
import {
  type CodeBlock,
  type DiscoveredClientExtension,
  type ExtensionHostContext,
  type MessageExtensionHostContext,
  type RegisteredChatAction,
  type RegisteredContentSuffix,
  type RegisteredCustomRender,
  type RegisteredRootLink,
  type RegisteredSidecar,
} from './types';
import { evaluateWhenClause } from './when';
import { clientExtensionViewPath } from './routes';
import { selectCustomRender } from './customRender';
import { buildSandboxedDocument } from './sandbox';
import { registerPluginThemes } from '../theme/theme-tokens';
import {
  acpInstallClientExtension,
  acpListClientExtensions,
  acpReadClientExtensionMain,
  acpSetClientExtensionEnabled,
  acpUninstallClientExtension,
  type ClientExtensionsListing,
} from '../acp/clientExtensions';

interface ClientExtensionsContextValue {
  extensions: DiscoveredClientExtension[];
  installDir: string | null;
  loading: boolean;
  registryVersion: number;
  getChatActions: (context: ExtensionHostContext) => RegisteredChatAction[];
  getRootLinks: (context: ExtensionHostContext) => RegisteredRootLink[];
  getContentSuffixes: (context: MessageExtensionHostContext) => RegisteredContentSuffix[];
  getCustomRender: (
    context: MessageExtensionHostContext,
    codeBlocks: CodeBlock[]
  ) => RegisteredCustomRender | null;
  getSidecars: (context: ExtensionHostContext) => RegisteredSidecar[];
  getExtensionFrameDocument: (extensionId: string) => Promise<string | null>;
  reloadExtensions: () => Promise<void>;
  setExtensionEnabled: (extensionId: string, enabled: boolean) => Promise<void>;
  uninstallExtension: (extensionId: string) => Promise<void>;
  installExtension: (sourcePath: string) => Promise<void>;
}

function collectContributions<C extends { when?: string }, R>(
  extensions: DiscoveredClientExtension[],
  getContributions: (ext: DiscoveredClientExtension) => C[],
  context: ExtensionHostContext | MessageExtensionHostContext,
  enrich: (contribution: C, extensionId: string) => R
): R[] {
  const results: R[] = [];
  for (const extension of extensions) {
    for (const contribution of getContributions(extension)) {
      if (!evaluateWhenClause(contribution.when, context)) continue;
      results.push(enrich(contribution, extension.id));
    }
  }
  return results;
}

const ClientExtensionsContext = createContext<ClientExtensionsContextValue | null>(null);

export function ClientExtensionsProvider({ children }: { children: React.ReactNode }) {
  const [extensions, setExtensions] = useState<DiscoveredClientExtension[]>([]);
  const [installDir, setInstallDir] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [registryVersion, setRegistryVersion] = useState(0);

  const applyListing = useCallback((listing: ClientExtensionsListing) => {
    setExtensions(listing.extensions);
    setInstallDir(listing.installDir);
    setRegistryVersion((version) => version + 1);
  }, []);

  const reloadExtensions = useCallback(async () => {
    try {
      applyListing(await acpListClientExtensions());
    } catch (error) {
      console.warn('[client-extensions] Failed to discover extensions:', error);
      setExtensions([]);
      setRegistryVersion((version) => version + 1);
    } finally {
      setLoading(false);
    }
  }, [applyListing]);

  const enabledExtensions = useMemo(
    () => extensions.filter((extension) => extension.enabled),
    [extensions]
  );

  useEffect(() => {
    registerPluginThemes(
      enabledExtensions.flatMap((extension) =>
        (extension.manifest.contributes?.themes ?? []).map((theme) => ({
          ...theme,
          extensionId: extension.id,
        }))
      )
    );
  }, [enabledExtensions]);

  const setExtensionEnabled = useCallback(
    async (extensionId: string, enabled: boolean) => {
      try {
        applyListing(await acpSetClientExtensionEnabled(extensionId, enabled));
      } catch (error) {
        console.warn('[client-extensions] Failed to update extension state:', error);
        throw error;
      }
    },
    [applyListing]
  );

  const uninstallExtension = useCallback(
    async (extensionId: string) => {
      try {
        applyListing(await acpUninstallClientExtension(extensionId));
      } catch (error) {
        console.warn('[client-extensions] Failed to uninstall extension:', error);
        throw error;
      }
    },
    [applyListing]
  );

  const installExtension = useCallback(
    async (sourcePath: string) => {
      try {
        applyListing(await acpInstallClientExtension(sourcePath));
      } catch (error) {
        console.warn('[client-extensions] Failed to install extension:', error);
        throw error;
      }
    },
    [applyListing]
  );

  useEffect(() => {
    void reloadExtensions();
  }, [reloadExtensions]);

  const getExtensionFrameDocument = useCallback(async (extensionId: string) => {
    try {
      return buildSandboxedDocument(await acpReadClientExtensionMain(extensionId));
    } catch (error) {
      console.warn(`[client-extensions] Failed to read main for "${extensionId}":`, error);
      return null;
    }
  }, []);

  const getChatActions = useCallback(
    (context: ExtensionHostContext): RegisteredChatAction[] =>
      collectContributions(
        enabledExtensions,
        (ext) => ext.manifest.contributes?.chatActions ?? [],
        context,
        (c, id) => ({ ...c, extensionId: id })
      ),
    [enabledExtensions]
  );

  const getRootLinks = useCallback(
    (context: ExtensionHostContext): RegisteredRootLink[] =>
      collectContributions(
        enabledExtensions,
        (ext) => ext.manifest.contributes?.rootLinks ?? [],
        context,
        (c, id) => ({ ...c, extensionId: id, path: clientExtensionViewPath(id, c.id) })
      ),
    [enabledExtensions]
  );

  const getContentSuffixes = useCallback(
    (context: MessageExtensionHostContext): RegisteredContentSuffix[] =>
      collectContributions(
        enabledExtensions,
        (ext) => ext.manifest.contributes?.contentSuffixes ?? [],
        context,
        (c, id) => ({ ...c, extensionId: id })
      ),
    [enabledExtensions]
  );

  const getCustomRender = useCallback(
    (
      context: MessageExtensionHostContext,
      codeBlocks: CodeBlock[]
    ): RegisteredCustomRender | null => {
      const renders: RegisteredCustomRender[] = [];
      for (const extension of enabledExtensions) {
        for (const contribution of extension.manifest.contributes?.customRenders ?? []) {
          renders.push({ ...contribution, extensionId: extension.id });
        }
      }
      return selectCustomRender(renders, context, codeBlocks);
    },
    [enabledExtensions]
  );

  const getSidecars = useCallback(
    (context: ExtensionHostContext): RegisteredSidecar[] =>
      collectContributions(
        enabledExtensions,
        (ext) => ext.manifest.contributes?.sidecars ?? [],
        context,
        (c, id) => ({ ...c, extensionId: id })
      ),
    [enabledExtensions]
  );

  const value = useMemo(
    () => ({
      extensions,
      installDir,
      loading,
      registryVersion,
      getChatActions,
      getRootLinks,
      getContentSuffixes,
      getCustomRender,
      getSidecars,
      getExtensionFrameDocument,
      reloadExtensions,
      setExtensionEnabled,
      uninstallExtension,
      installExtension,
    }),
    [
      extensions,
      installDir,
      loading,
      registryVersion,
      getChatActions,
      getRootLinks,
      getContentSuffixes,
      getCustomRender,
      getSidecars,
      getExtensionFrameDocument,
      reloadExtensions,
      setExtensionEnabled,
      uninstallExtension,
      installExtension,
    ]
  );

  return (
    <ClientExtensionsContext.Provider value={value}>{children}</ClientExtensionsContext.Provider>
  );
}

export function useClientExtensions(): ClientExtensionsContextValue {
  const context = useContext(ClientExtensionsContext);
  if (!context) {
    throw new Error('useClientExtensions must be used within ClientExtensionsProvider');
  }
  return context;
}

export function useExtensionHostContext(sessionId: string | null): ExtensionHostContext {
  const location = useLocation();
  return useMemo(
    () => ({
      sessionId,
      route: location.pathname,
    }),
    [sessionId, location.pathname]
  );
}
