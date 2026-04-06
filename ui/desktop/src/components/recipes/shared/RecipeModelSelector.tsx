import { useEffect, useState, useCallback } from 'react';
import { Select } from '../../ui/Select';
import { Input } from '../../ui/input';
import { useConfig } from '../../ConfigContext';
import { fetchModelsForProviders } from '../../settings/models/modelInterface';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  fetchError: {
    id: 'recipeModelSelector.fetchError',
    defaultMessage: 'Failed to fetch models. Please try again later.',
  },
  providerLabel: {
    id: 'recipeModelSelector.providerLabel',
    defaultMessage: 'Provider (Optional)',
  },
  providerHint: {
    id: 'recipeModelSelector.providerHint',
    defaultMessage: 'Leave empty to use the default provider configured in settings',
  },
  selectProvider: {
    id: 'recipeModelSelector.selectProvider',
    defaultMessage: 'Select provider',
  },
  useDefaultProvider: {
    id: 'recipeModelSelector.useDefaultProvider',
    defaultMessage: 'Use default provider',
  },
  enterModelNotListed: {
    id: 'recipeModelSelector.enterModelNotListed',
    defaultMessage: 'Enter a model not listed...',
  },
  modelLabel: {
    id: 'recipeModelSelector.modelLabel',
    defaultMessage: 'Model (Optional)',
  },
  backToModelList: {
    id: 'recipeModelSelector.backToModelList',
    defaultMessage: 'Back to model list',
  },
  modelHint: {
    id: 'recipeModelSelector.modelHint',
    defaultMessage: 'Leave empty to use the default model for the selected provider',
  },
  enterCustomModel: {
    id: 'recipeModelSelector.enterCustomModel',
    defaultMessage: 'Enter custom model name',
  },
  loadingModels: {
    id: 'recipeModelSelector.loadingModels',
    defaultMessage: 'Loading models…',
  },
  selectModel: {
    id: 'recipeModelSelector.selectModel',
    defaultMessage: 'Select a model',
  },
});

interface RecipeModelSelectorProps {
  selectedProvider?: string;
  selectedModel?: string;
  onProviderChange: (provider: string | undefined) => void;
  onModelChange: (model: string | undefined) => void;
}

export const RecipeModelSelector = ({
  selectedProvider,
  selectedModel,
  onProviderChange,
  onModelChange,
}: RecipeModelSelectorProps) => {
  const intl = useIntl();
  const { getProviders } = useConfig();
  const [providerOptions, setProviderOptions] = useState<{ value: string; label: string }[]>([]);
  const [modelOptions, setModelOptions] = useState<
    { options: { value: string; label: string; provider: string }[] }[]
  >([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [isCustomModel, setIsCustomModel] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        setFetchError(null);
        const providersResponse = await getProviders(false);
        const activeProviders = providersResponse.filter((provider) => provider.is_configured);

        setProviderOptions([
          { value: '', label: intl.formatMessage(i18n.useDefaultProvider) },
          ...activeProviders.map(({ metadata, name }) => ({
            value: name,
            label: metadata.display_name,
          })),
        ]);

        setLoadingModels(true);
        const results = await fetchModelsForProviders(activeProviders);

        const groupedOptions: {
          options: { value: string; label: string; provider: string }[];
        }[] = [];

        results.forEach(({ provider: p, models, error }) => {
          if (error) {
            return;
          }

          const modelList = models || [];
          const options = modelList.map((m) => ({
            value: m,
            label: m,
            provider: p.name,
          }));

          options.push({
            value: `__custom__:${p.name}`,
            label: intl.formatMessage(i18n.enterModelNotListed),
            provider: p.name,
          });

          if (options.length > 0) {
            groupedOptions.push({ options });
          }
        });

        setModelOptions(groupedOptions);
      } catch (error) {
        console.error('Failed to load providers:', error);
        setFetchError(intl.formatMessage(i18n.fetchError));
      } finally {
        setLoadingModels(false);
      }
    })();
  }, [getProviders, intl]);

  useEffect(() => {
    if (!loadingModels && selectedModel && selectedProvider) {
      const allModels = modelOptions.flatMap((group) => group.options);
      const modelExists = allModels.some(
        (opt) => opt.value === selectedModel && opt.provider === selectedProvider
      );
      if (!modelExists) {
        setIsCustomModel(true);
      }
    }
  }, [loadingModels, modelOptions, selectedModel, selectedProvider]);

  const filteredModelOptions = selectedProvider
    ? modelOptions.filter((group) => group.options[0]?.provider === selectedProvider)
    : [];

  const handleProviderChange = useCallback(
    (newValue: unknown) => {
      const option = newValue as { value: string; label: string } | null;
      const providerValue = option?.value || undefined;
      onProviderChange(providerValue === '' ? undefined : providerValue);
      onModelChange(undefined);
      setIsCustomModel(false);
    },
    [onProviderChange, onModelChange]
  );

  const handleModelChange = useCallback(
    (newValue: unknown) => {
      const option = newValue as { value: string; label: string; provider: string } | null;
      if (option?.value.startsWith('__custom__:')) {
        setIsCustomModel(true);
        onModelChange(undefined);
      } else {
        setIsCustomModel(false);
        onModelChange(option?.value || undefined);
      }
    },
    [onModelChange]
  );

  return (
    <div className="space-y-4">
      {fetchError && (
        <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
          {fetchError}
        </div>
      )}
      <div>
        <label className="block text-sm font-medium text-textStandard mb-2">
          {intl.formatMessage(i18n.providerLabel)}
        </label>
        <p className="text-xs text-textSubtle mb-2">
          {intl.formatMessage(i18n.providerHint)}
        </p>
        <Select
          options={providerOptions}
          value={
            selectedProvider
              ? providerOptions.find((opt) => opt.value === selectedProvider) || null
              : providerOptions.find((opt) => opt.value === '') || null
          }
          onChange={handleProviderChange}
          placeholder={intl.formatMessage(i18n.selectProvider)}
          isClearable
        />
      </div>

      <div>
        <div className="flex justify-between items-center mb-2">
          <label className="block text-sm font-medium text-textStandard">{intl.formatMessage(i18n.modelLabel)}</label>
          {isCustomModel && (
            <button
              onClick={() => {
                setIsCustomModel(false);
                onModelChange(undefined);
              }}
              className="text-xs text-textSubtle hover:underline"
              type="button"
            >
              {intl.formatMessage(i18n.backToModelList)}
            </button>
          )}
        </div>
        <p className="text-xs text-textSubtle mb-2">
          {intl.formatMessage(i18n.modelHint)}
        </p>
        {isCustomModel ? (
          <Input
            type="text"
            placeholder={intl.formatMessage(i18n.enterCustomModel)}
            value={selectedModel || ''}
            onChange={(e) => onModelChange(e.target.value || undefined)}
          />
        ) : (
          <Select
            options={loadingModels ? [] : filteredModelOptions}
            value={
              loadingModels
                ? { value: '', label: intl.formatMessage(i18n.loadingModels), isDisabled: true }
                : selectedModel
                  ? { value: selectedModel, label: selectedModel }
                  : null
            }
            onChange={handleModelChange}
            placeholder={intl.formatMessage(i18n.selectModel)}
            isClearable
            isDisabled={loadingModels}
          />
        )}
      </div>
    </div>
  );
};
