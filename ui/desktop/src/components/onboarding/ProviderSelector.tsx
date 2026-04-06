import { useState, useEffect, useMemo } from 'react';
import {
  providers as fetchProviders,
  createCustomProvider,
  ProviderDetails,
  UpdateCustomProviderRequest,
} from '../../api';
import { Select } from '../ui/Select';
import ProviderConfigForm from './ProviderConfigForm';
import FreeOptionCards from './FreeOptionCards';
import CustomProviderForm from '../settings/providers/modal/subcomponents/forms/CustomProviderForm';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { Gift, Key, Plus } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  useFreeLocal: {
    id: 'providerSelector.useFreeLocal',
    defaultMessage: 'Use Free/Local Providers',
  },
  freeLocalDescription: {
    id: 'providerSelector.freeLocalDescription',
    defaultMessage: 'Use a local model or a provider with free credits',
  },
  connectProvider: {
    id: 'providerSelector.connectProvider',
    defaultMessage: 'Connect to a Provider',
  },
  connectProviderDescription: {
    id: 'providerSelector.connectProviderDescription',
    defaultMessage: 'Connect OpenAI, Anthropic, Google, etc',
  },
  selectProvider: {
    id: 'providerSelector.selectProvider',
    defaultMessage: 'Select a provider',
  },
  addCustomProvider: {
    id: 'providerSelector.addCustomProvider',
    defaultMessage: 'Add a custom provider',
  },
  addCustomProviderTitle: {
    id: 'providerSelector.addCustomProviderTitle',
    defaultMessage: 'Add Custom Provider',
  },
});

const FREE_OPTIONS = 'free-options' as const;
const OWN_PROVIDER = 'own-provider' as const;

type SelectedPath = typeof FREE_OPTIONS | typeof OWN_PROVIDER | null;

interface ProviderOption {
  value: string;
  label: string;
  provider: ProviderDetails;
}

interface ProviderSelectorProps {
  onConfigured: (providerName: string, modelId?: string) => void;
  onFirstSelection?: () => void;
}

export default function ProviderSelector({
  onConfigured,
  onFirstSelection,
}: ProviderSelectorProps) {
  const intl = useIntl();
  const [providerList, setProviderList] = useState<ProviderDetails[]>([]);
  const [selectedOption, setSelectedOption] = useState<ProviderOption | null>(null);
  const [selectedPath, setSelectedPath] = useState<SelectedPath>(null);
  const [showCustomModal, setShowCustomModal] = useState(false);

  useEffect(() => {
    const load = async () => {
      try {
        const response = await fetchProviders({ throwOnError: true });
        if (response.data) {
          const list = Array.isArray(response.data)
            ? response.data
            : (response.data as { providers: ProviderDetails[] }).providers || [];
          setProviderList(list);
        }
      } catch (err) {
        console.error('Failed to fetch providers:', err);
      }
    };
    load();
  }, []);

  const options: ProviderOption[] = useMemo(() => {
    return [...providerList]
      .sort((a, b) => {
        const aPreferred = a.provider_type === 'Preferred' ? 0 : 1;
        const bPreferred = b.provider_type === 'Preferred' ? 0 : 1;
        if (aPreferred !== bPreferred) return aPreferred - bPreferred;
        return a.metadata.display_name.localeCompare(b.metadata.display_name);
      })
      .map((provider) => ({
        value: provider.name,
        label: provider.metadata.display_name,
        provider,
      }));
  }, [providerList]);

  const fuzzyFilterOption = (option: { label: string; value: string }, inputValue: string) => {
    const normalize = (s: string) => s.toLowerCase().replace(/[\s_-]/g, '');
    return (
      normalize(option.label).includes(normalize(inputValue)) ||
      normalize(option.value).includes(normalize(inputValue))
    );
  };

  const handleFreeCreditClick = () => {
    setSelectedPath(FREE_OPTIONS);
    setSelectedOption(null);
    onFirstSelection?.();
  };

  const handleOwnProviderClick = () => {
    setSelectedPath(OWN_PROVIDER);
    onFirstSelection?.();
  };

  const handleProviderSelect = (option: ProviderOption | null) => {
    setSelectedOption(option);
    if (option) onFirstSelection?.();
  };

  const handleCreateCustomProvider = async (data: UpdateCustomProviderRequest) => {
    const result = await createCustomProvider({ body: data, throwOnError: true });
    setShowCustomModal(false);
    if (result.data?.provider_name) {
      onConfigured(result.data.provider_name);
    }
  };

  const selectedProvider = selectedOption?.provider ?? null;

  return (
    <div>
      <div className="grid grid-cols-2 gap-3 mb-6">
        <div
          onClick={handleFreeCreditClick}
          className={`p-4 border rounded-xl transition-all duration-200 cursor-pointer group ${
            selectedPath === FREE_OPTIONS
              ? 'border-blue-400 bg-background-muted'
              : 'border-border-default bg-background-muted hover:border-blue-400'
          }`}
        >
          <Gift size={20} className="text-text-muted mb-2" />
          <span className="font-medium text-text-default text-base block">
            {intl.formatMessage(i18n.useFreeLocal)}
          </span>
          <p className="text-text-muted text-sm mt-1">
            {intl.formatMessage(i18n.freeLocalDescription)}
          </p>
        </div>

        <div
          onClick={handleOwnProviderClick}
          className={`p-4 border rounded-xl transition-all duration-200 cursor-pointer group ${
            selectedPath === OWN_PROVIDER
              ? 'border-blue-400 bg-background-muted'
              : 'border-border-default bg-background-muted hover:border-blue-400'
          }`}
        >
          <Key size={20} className="text-text-muted mb-2" />
          <span className="font-medium text-text-default text-base block">
            {intl.formatMessage(i18n.connectProvider)}
          </span>
          <p className="text-text-muted text-sm mt-1">{intl.formatMessage(i18n.connectProviderDescription)}</p>
        </div>
      </div>

      {selectedPath === FREE_OPTIONS && (
        <div className="animate-in fade-in slide-in-from-top-2 duration-300">
          <FreeOptionCards onConfigured={onConfigured} />
        </div>
      )}

      {selectedPath === OWN_PROVIDER && (
        <div className="animate-in fade-in slide-in-from-top-2 duration-300">
          <div className="mb-4">
            <Select
              options={options}
              value={selectedOption}
              onChange={(option) => handleProviderSelect(option as ProviderOption | null)}
              placeholder={intl.formatMessage(i18n.selectProvider)}
              isClearable
              isSearchable
              autoFocus
              filterOption={fuzzyFilterOption}
            />
          </div>

          <button
            onClick={() => setShowCustomModal(true)}
            className="flex items-center gap-1 text-sm text-text-muted hover:text-text-default transition-colors mb-6"
          >
            <Plus size={14} />
            <span>{intl.formatMessage(i18n.addCustomProvider)}</span>
          </button>

          {selectedProvider && (
            <ProviderConfigForm
              key={selectedProvider.name}
              provider={selectedProvider}
              onConfigured={onConfigured}
            />
          )}
        </div>
      )}

      <Dialog open={showCustomModal} onOpenChange={setShowCustomModal}>
        <DialogContent className="sm:max-w-[600px] max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{intl.formatMessage(i18n.addCustomProviderTitle)}</DialogTitle>
          </DialogHeader>
          <CustomProviderForm
            initialData={null}
            isEditable={true}
            onSubmit={handleCreateCustomProvider}
            onCancel={() => setShowCustomModal(false)}
          />
        </DialogContent>
      </Dialog>
    </div>
  );
}
