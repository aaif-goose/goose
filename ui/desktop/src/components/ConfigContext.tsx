import React, { createContext, useContext, useState, useEffect, useMemo, useCallback } from 'react';
import {
  readAllConfig,
  readConfig,
  removeConfig,
  upsertConfig,
  getExtensions as apiGetExtensions,
  addExtension as apiAddExtension,
  removeExtension as apiRemoveExtension,
  providers,
} from '../api';
import { syncBundledExtensions } from './settings/extensions';
import type {
  ConfigResponse,
  UpsertConfigQuery,
  ConfigKeyQuery,
  ExtensionResponse,
  ProviderDetails,
  ExtensionQuery,
  ExtensionConfig,
} from '../api';

export type { ExtensionConfig } from '../api/types.gen';

// Define a local version that matches the structure of the imported one
export type FixedExtensionEntry = ExtensionConfig & {
  enabled: boolean;
};

interface ConfigContextType {
  config: ConfigResponse['config'];
  providersList: ProviderDetails[];
  extensionsList: FixedExtensionEntry[];
  extensionWarnings: string[];
  upsert: (key: string, value: unknown, is_secret: boolean) => Promise<void>;
  read: (key: string, is_secret: boolean, options?: { throwOnError?: boolean }) => Promise<unknown>;
  remove: (key: string, is_secret: boolean) => Promise<void>;
  addExtension: (name: string, config: ExtensionConfig, enabled: boolean) => Promise<void>;
  toggleExtension: (name: string) => Promise<void>;
  removeExtension: (name: string) => Promise<void>;
  getProviders: (b: boolean) => Promise<ProviderDetails[]>;
  getExtensions: (b: boolean) => Promise<FixedExtensionEntry[]>;
  disableAllExtensions: () => Promise<void>;
  enableBotExtensions: (extensions: ExtensionConfig[]) => Promise<void>;
}

interface ConfigProviderProps {
  children: React.ReactNode;
}

export class MalformedConfigError extends Error {
  constructor() {
    super('Check contents of ~/.config/goose/config.yaml');
    this.name = 'MalformedConfigError';
    Object.setPrototypeOf(this, MalformedConfigError.prototype);
  }
}

const ConfigContext = createContext<ConfigContextType | undefined>(undefined);

export const ConfigProvider: React.FC<ConfigProviderProps> = ({ children }) => {
  const [config, setConfig] = useState<ConfigResponse['config']>({});
  const [providersList, setProvidersList] = useState<ProviderDetails[]>([]);
  const [extensionsList, setExtensionsList] = useState<FixedExtensionEntry[]>([]);
  const [extensionWarnings, setExtensionWarnings] = useState<string[]>([]);

  // Ref to access providersList in getProviders without recreating the callback
  const providersListRef = React.useRef<ProviderDetails[]>(providersList);
  providersListRef.current = providersList;

  const reloadConfig = useCallback(async () => {
    const response = await readAllConfig();
    setConfig(response.data?.config || {});
  }, []);

  const upsert = useCallback(
    async (key: string, value: unknown, isSecret: boolean = false) => {
      const query: UpsertConfigQuery = {
        key: key,
        value: value,
        is_secret: isSecret,
      };
      await upsertConfig({
        body: query,
      });
      await reloadConfig();
    },
    [reloadConfig]
  );

  const read = useCallback(async (key: string, is_secret: boolean = false, options?: { throwOnError?: boolean }) => {
    const query: ConfigKeyQuery = { key: key, is_secret: is_secret };
    const response = await readConfig({
      body: query,
    });
    if (options?.throwOnError && response.error) {
      throw response.error;
    }
    return response.data;
  }, []);

  const remove = useCallback(
    async (key: string, is_secret: boolean) => {
      const query: ConfigKeyQuery = { key: key, is_secret: is_secret };
      await removeConfig({
        body: query,
      });
      await reloadConfig();
    },
    [reloadConfig]
  );

  const refreshExtensions = useCallback(async () => {
    const result = await apiGetExtensions();

    if (result.response.status === 422) {
      throw new MalformedConfigError();
    }

    if (result.error && !result.data) {
      console.error(result.error);
      return extensionsList;
    }

    const extensionResponse: ExtensionResponse = result.data!;
    setExtensionsList(extensionResponse.extensions);
    setExtensionWarnings(extensionResponse.warnings || []);
    return extensionResponse.extensions;
  }, [extensionsList]);

  const addExtension = useCallback(
    async (name: string, config: ExtensionConfig, enabled: boolean) => {
      const query: ExtensionQuery = { name, config, enabled };
      await apiAddExtension({
        body: query,
      });
      await reloadConfig();
      // Refresh extensions list after successful addition
      await refreshExtensions();
    },
    [reloadConfig, refreshExtensions]
  );

  const removeExtension = useCallback(
    async (name: string) => {
      await apiRemoveExtension({ path: { name: name } });
      await reloadConfig();
      // Refresh extensions list after successful removal
      await refreshExtensions();
    },
    [reloadConfig, refreshExtensions]
  );

  const getExtensions = useCallback(
    async (forceRefresh = false): Promise<FixedExtensionEntry[]> => {
      if (forceRefresh || extensionsList.length === 0) {
        return await refreshExtensions();
      }
      return extensionsList;
    },
    [extensionsList, refreshExtensions]
  );

  const toggleExtension = useCallback(
    async (name: string) => {
      const exts = await getExtensions(true);
      const extension = exts.find((ext) => ext.name === name);

      if (extension) {
        await addExtension(name, extension, !extension.enabled);
      }
    },
    [addExtension, getExtensions]
  );

  const getProviders = useCallback(async (forceRefresh = false): Promise<ProviderDetails[]> => {
    if (forceRefresh || providersListRef.current.length === 0) {
      try {
        const response = await providers();
        const providersData = response.data || [];
        setProvidersList(providersData);
        return providersData;
      } catch (error) {
        console.error('Failed to fetch providers:', error);
        return providersListRef.current;
      }
    }
    return providersListRef.current;
  }, []);

  useEffect(() => {
    // Load all configuration data and providers on mount
    (async () => {
      // Load config
      const configResponse = await readAllConfig();
      setConfig(configResponse.data?.config || {});

      // Load providers
      try {
        const providersResponse = await providers();
        const providersData = providersResponse.data || [];
        setProvidersList(providersData);
      } catch (error) {
        console.error('Failed to load providers:', error);
        setProvidersList([]);
      }

      // Load extensions
      try {
        const extensionsResponse = await apiGetExtensions();
        let extensions = extensionsResponse.data?.extensions || [];

        // Always sync bundled extensions from bundled-extensions.json
        // This ensures:
        // 1. Fresh installs get the default extensions (developer, computercontroller, etc.)
        // 2. Existing users get NEW bundled extensions added in subsequent releases
        // The syncBundledExtensions function skips extensions that already exist and are marked as bundled
        // Platform extensions (code_execution, todo, etc.) are handled by the backend
        const addExtensionForSync = async (
          name: string,
          config: ExtensionConfig,
          enabled: boolean
        ) => {
          const query: ExtensionQuery = { name, config, enabled };
          await apiAddExtension({ body: query });
        };
        await syncBundledExtensions(extensions, addExtensionForSync);
        // Reload extensions after sync
        const refreshedResponse = await apiGetExtensions();
        extensions = refreshedResponse.data?.extensions || [];

        setExtensionsList(extensions);
        setExtensionWarnings(extensionsResponse.data?.warnings || []);
      } catch (error) {
        console.error('Failed to load extensions:', error);
      }
    })();
  }, []);

  const contextValue = useMemo(() => {
    const disableAllExtensions = async () => {
      const currentExtensions = await getExtensions(true);
      for (const ext of currentExtensions) {
        if (ext.enabled) {
          await addExtension(ext.name, ext, false);
        }
      }
      await reloadConfig();
    };

    const enableBotExtensions = async (extensions: ExtensionConfig[]) => {
      for (const ext of extensions) {
        await addExtension(ext.name, ext, true);
      }
      await reloadConfig();
    };

    return {
      config,
      providersList,
      extensionsList,
      extensionWarnings,
      upsert,
      read,
      remove,
      addExtension,
      removeExtension,
      toggleExtension,
      getProviders,
      getExtensions,
      disableAllExtensions,
      enableBotExtensions,
    };
  }, [
    config,
    providersList,
    extensionsList,
    extensionWarnings,
    upsert,
    read,
    remove,
    addExtension,
    removeExtension,
    toggleExtension,
    getProviders,
    getExtensions,
    reloadConfig,
  ]);

  return <ConfigContext.Provider value={contextValue}>{children}</ConfigContext.Provider>;
};

export const useConfig = () => {
  const context = useContext(ConfigContext);
  if (context === undefined) {
    throw new Error('useConfig must be used within a ConfigProvider');
  }
  return context;
};
