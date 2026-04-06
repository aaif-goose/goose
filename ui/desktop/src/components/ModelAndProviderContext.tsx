import React, { createContext, useContext, useState, useEffect, useMemo, useCallback } from 'react';
import { toastError, toastSuccess } from '../toasts';
import Model, { getProviderMetadata } from './settings/models/modelInterface';
import { ProviderMetadata, setConfigProvider, updateAgentProvider } from '../api';
import { useConfig } from './ConfigContext';
import { errorMessage } from '../utils/conversionUtils';
import {
  getModelDisplayName,
  getProviderDisplayName,
} from './settings/models/predefinedModelsUtils';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  unknownProviderTitle: {
    id: 'modelAndProviderContext.unknownProviderTitle',
    defaultMessage: 'Provider name lookup',
  },
  unknownProviderMsg: {
    id: 'modelAndProviderContext.unknownProviderMsg',
    defaultMessage: 'Unknown provider in config -- please inspect your config.yaml',
  },
  modelChangedTitle: {
    id: 'modelAndProviderContext.modelChangedTitle',
    defaultMessage: 'Model changed',
  },
  switchModelSuccess: {
    id: 'modelAndProviderContext.switchModelSuccess',
    defaultMessage: 'Successfully switched models -- using {model} from {provider}',
  },
  modelChangeFailed: {
    id: 'modelAndProviderContext.modelChangeFailed',
    defaultMessage: '{provider}/{model} failed',
  },
  selectModel: {
    id: 'modelAndProviderContext.selectModel',
    defaultMessage: 'Select Model',
  },
});

interface ModelAndProviderContextType {
  currentModel: string | null;
  currentProvider: string | null;
  changeModel: (sessionId: string | null, model: Model) => Promise<boolean>;
  getCurrentModelAndProvider: () => Promise<{ model: string; provider: string }>;
  getFallbackModelAndProvider: () => Promise<{ model: string; provider: string }>;
  getCurrentModelAndProviderForDisplay: () => Promise<{ model: string; provider: string }>;
  getCurrentModelDisplayName: () => Promise<string>;
  getCurrentProviderDisplayName: () => Promise<string>; // Gets provider display name from subtext
  refreshCurrentModelAndProvider: () => Promise<void>;
}

interface ModelAndProviderProviderProps {
  children: React.ReactNode;
}

const ModelAndProviderContext = createContext<ModelAndProviderContextType | undefined>(undefined);

export { i18n as modelAndProviderMessages };

export const ModelAndProviderProvider: React.FC<ModelAndProviderProviderProps> = ({ children }) => {
  const [currentModel, setCurrentModel] = useState<string | null>(null);
  const [currentProvider, setCurrentProvider] = useState<string | null>(null);
  const { read, getProviders } = useConfig();
  const intl = useIntl();

  const changeModel = useCallback(async (sessionId: string | null, model: Model) => {
    const modelName = model.name;
    const providerName = model.provider;
    let phase = 'agent';

    try {
      if (sessionId) {
        const response = await updateAgentProvider({
          body: {
            session_id: sessionId,
            provider: providerName,
            model: modelName,
            context_limit: model.context_limit,
            request_params: model.request_params,
          },
        });
        if (response.error) {
          throw new Error(`Failed to update agent provider: ${response.error}`);
        }
      }

      // Only update the global config default when there's no session
      // (i.e. changing from settings, not from within an existing chat)
      if (!sessionId) {
        phase = 'config';
        await setConfigProvider({
          body: {
            provider: providerName,
            model: modelName,
          },
          throwOnError: true,
        });
      }

      if (!sessionId) {
        setCurrentProvider(providerName);
        setCurrentModel(modelName);
      }

      toastSuccess({
        title: intl.formatMessage(i18n.modelChangedTitle),
        msg: intl.formatMessage(i18n.switchModelSuccess, {
          model: model.alias ?? modelName,
          provider: model.subtext ?? providerName,
        }),
      });
      return true;
    } catch (error) {
      console.error(`Failed to change model at ${phase} step -- ${modelName} ${providerName}`);
      toastError({
        title: intl.formatMessage(i18n.modelChangeFailed, { provider: providerName, model: modelName }),
        msg: `${error}`,
        traceback: errorMessage(error),
      });
      return false;
    }
  }, [intl]);

  const getFallbackModelAndProvider = useCallback(async () => {
    const provider = window.appConfig.get('GOOSE_DEFAULT_PROVIDER') as string;
    const model = window.appConfig.get('GOOSE_DEFAULT_MODEL') as string;
    if (provider && model) {
      try {
        await setConfigProvider({
          body: {
            provider: provider,
            model: model,
          },
          throwOnError: true,
        });
      } catch (error) {
        console.error('[getFallbackModelAndProvider] Failed to write to config', error);
      }
    }
    return { model: model, provider: provider };
  }, []);

  const getCurrentModelAndProvider = useCallback(async () => {
    let model: string;
    let provider: string;

    // read from config
    try {
      model = (await read('GOOSE_MODEL', false)) as string;
      provider = (await read('GOOSE_PROVIDER', false)) as string;
    } catch {
      console.error(`Failed to read GOOSE_MODEL or GOOSE_PROVIDER from config`);
      throw new Error('Failed to read GOOSE_MODEL or GOOSE_PROVIDER from config');
    }
    if (!model || !provider) {
      return getFallbackModelAndProvider();
    }
    return { model: model, provider: provider };
  }, [read, getFallbackModelAndProvider]);

  const getCurrentModelAndProviderForDisplay = useCallback(async () => {
    const modelProvider = await getCurrentModelAndProvider();
    const gooseModel = modelProvider.model;
    const gooseProvider = modelProvider.provider;

    // lookup display name
    let metadata: ProviderMetadata;

    try {
      metadata = await getProviderMetadata(String(gooseProvider), getProviders);
    } catch {
      return { model: gooseModel, provider: gooseProvider };
    }
    const providerDisplayName = metadata.display_name;

    return { model: gooseModel, provider: providerDisplayName };
  }, [getCurrentModelAndProvider, getProviders]);

  const getCurrentModelDisplayName = useCallback(async () => {
    try {
      const currentModelName = (await read('GOOSE_MODEL', false)) as string;
      return getModelDisplayName(currentModelName);
    } catch {
      return intl.formatMessage(i18n.selectModel);
    }
  }, [read, intl]);

  const getCurrentProviderDisplayName = useCallback(async () => {
    try {
      const currentModelName = (await read('GOOSE_MODEL', false)) as string;
      const providerDisplayName = getProviderDisplayName(currentModelName);
      if (providerDisplayName) {
        return providerDisplayName;
      }
      // Fall back to regular provider display name lookup
      const { provider } = await getCurrentModelAndProviderForDisplay();
      return provider;
    } catch {
      return '';
    }
  }, [read, getCurrentModelAndProviderForDisplay]);

  const refreshCurrentModelAndProvider = useCallback(async () => {
    try {
      const { model, provider } = await getCurrentModelAndProvider();
      setCurrentModel(model);
      setCurrentProvider(provider);
    } catch (_error) {
      console.error('Failed to refresh current model and provider:', _error);
    }
  }, [getCurrentModelAndProvider]);

  // Load initial model and provider on mount
  useEffect(() => {
    refreshCurrentModelAndProvider();
  }, [refreshCurrentModelAndProvider]);

  const contextValue = useMemo(
    () => ({
      currentModel,
      currentProvider,
      changeModel,
      getCurrentModelAndProvider,
      getFallbackModelAndProvider,
      getCurrentModelAndProviderForDisplay,
      getCurrentModelDisplayName,
      getCurrentProviderDisplayName,
      refreshCurrentModelAndProvider,
    }),
    [
      currentModel,
      currentProvider,
      changeModel,
      getCurrentModelAndProvider,
      getFallbackModelAndProvider,
      getCurrentModelAndProviderForDisplay,
      getCurrentModelDisplayName,
      getCurrentProviderDisplayName,
      refreshCurrentModelAndProvider,
    ]
  );

  return (
    <ModelAndProviderContext.Provider value={contextValue}>
      {children}
    </ModelAndProviderContext.Provider>
  );
};

export const useModelAndProvider = () => {
  const context = useContext(ModelAndProviderContext);
  if (context === undefined) {
    throw new Error('useModelAndProvider must be used within a ModelAndProviderProvider');
  }
  return context;
};
