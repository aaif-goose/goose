import { Sliders, Bot, LoaderCircle, Settings } from 'lucide-react';
import React, { useEffect, useState } from 'react';
import { useModelAndProvider } from '../../../ModelAndProviderContext';
import { SwitchModelModal } from '../subcomponents/SwitchModelModal';
import { View } from '../../../../utils/navigationUtils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../../../ui/dropdown-menu';
import { useConfig } from '../../../ConfigContext';
import { getProviderMetadata } from '../modelInterface';
import { getModelDisplayName } from '../predefinedModelsUtils';

import { ModelSettingsPanel } from '../../localInference/ModelSettingsPanel';
import { ScrollArea } from '../../../ui/scroll-area';
import { defineMessages, useIntl } from '../../../../i18n';

const i18n = defineMessages({
  selectModel: {
    id: 'modelsBottomBar.selectModel',
    defaultMessage: 'Select Model',
  },
  currentModel: {
    id: 'modelsBottomBar.currentModel',
    defaultMessage: 'Current model',
  },
  loadingModel: {
    id: 'modelsBottomBar.loadingModel',
    defaultMessage: 'Loading model...',
  },
  changeModel: {
    id: 'modelsBottomBar.changeModel',
    defaultMessage: 'Change Model',
  },
  localModelSettings: {
    id: 'modelsBottomBar.localModelSettings',
    defaultMessage: 'Local Model Settings',
  },
  localModelSettingsTitle: {
    id: 'modelsBottomBar.localModelSettingsTitle',
    defaultMessage: 'Local Model Settings — {modelName}',
  },
});

interface ModelsBottomBarProps {
  sessionId: string | null;
  dropdownRef: React.RefObject<HTMLDivElement>;
  setView: (view: View) => void;
  sessionModel?: string | null;
  sessionProvider?: string | null;
  onModelChanged: (override: { model: string; provider: string }) => void;
  sessionLoaded?: boolean;
}

export default function ModelsBottomBar({
  sessionId,
  dropdownRef,
  setView,
  sessionModel,
  sessionProvider,
  onModelChanged,
  sessionLoaded,
}: ModelsBottomBarProps) {
  // ChatInput owns the override state and passes effective model/provider as sessionModel/sessionProvider.
  // Fall back to config defaults when no session-specific model is available.
  const { currentModel: configModel, currentProvider: configProvider } = useModelAndProvider();
  const currentModel = sessionModel ?? configModel;
  const currentProvider = sessionProvider ?? configProvider;

  const intl = useIntl();
  const { getProviders } = useConfig();
  const [displayProvider, setDisplayProvider] = useState<string | null>(null);
  const [displayModelName, setDisplayModelName] = useState<string>(intl.formatMessage(i18n.selectModel));
  const [isAddModelModalOpen, setIsAddModelModalOpen] = useState(false);
  const [isLocalModelSettingsOpen, setIsLocalModelSettingsOpen] = useState(false);
  const [providerDefaultModel, setProviderDefaultModel] = useState<string | null>(null);

  // Show a visible loading placeholder while session metadata is still being fetched,
  // rather than flashing the config default or leaving the footer blank.
  const isModelLoading = Boolean(sessionId && !sessionLoaded);
  const displayModel = currentModel || providerDefaultModel || displayModelName;
  const loadingModelLabel = intl.formatMessage(i18n.loadingModel);
  const triggerLabel = isModelLoading ? loadingModelLabel : displayModel;
  const menuModelLabel = isModelLoading ? loadingModelLabel : displayModelName;

  useEffect(() => {
    if (!currentProvider) return;
    getProviderMetadata(currentProvider, getProviders)
      .then((metadata) => {
        setDisplayProvider(metadata.display_name || currentProvider);
      })
      .catch(() => {
        setDisplayProvider(currentProvider);
      });
  }, [currentProvider, currentModel, getProviders]);

  // Fetch provider default model when provider changes and no current model
  useEffect(() => {
    if (currentProvider && !currentModel) {
      (async () => {
        try {
          const metadata = await getProviderMetadata(currentProvider, getProviders);
          setProviderDefaultModel(metadata.default_model);
        } catch (error) {
          console.error('Failed to get provider default model:', error);
          setProviderDefaultModel(null);
        }
      })();
    } else if (currentModel) {
      setProviderDefaultModel(null);
    }
  }, [currentProvider, currentModel, getProviders]);

  useEffect(() => {
    if (!currentModel) return;
    setDisplayModelName(getModelDisplayName(currentModel));
  }, [currentModel]);

  const handleModelSelected = (model: string, provider: string) => {
    onModelChanged({ model, provider });
  };

  return (
    <div className="relative flex items-center" ref={dropdownRef}>
      <DropdownMenu>
        <DropdownMenuTrigger className="flex items-center hover:cursor-pointer max-w-[180px] md:max-w-[200px] lg:max-w-[380px] min-w-0 text-text-primary/70 hover:text-text-primary transition-colors">
          <div className="flex items-center truncate max-w-[130px] md:max-w-[200px] lg:max-w-[360px] min-w-0">
            <Bot className="mr-1 h-4 w-4 flex-shrink-0" />
            {isModelLoading ? (
              <span
                data-testid="model-loading-state"
                className="inline-flex items-center gap-1 truncate text-xs"
              >
                <LoaderCircle className="h-3 w-3 animate-spin flex-shrink-0" />
                <span className="truncate">{triggerLabel}</span>
              </span>
            ) : (
              <span className="truncate text-xs">{triggerLabel}</span>
            )}
          </div>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="center" className="w-64 text-sm">
          <h6 className="text-xs text-text-primary mt-2 ml-2">{intl.formatMessage(i18n.currentModel)}</h6>
          <p className="flex items-center justify-between text-sm mx-2 pb-2 border-b mb-2">
            {menuModelLabel}
            {!isModelLoading && displayProvider && ` — ${displayProvider}`}
          </p>
          <DropdownMenuItem onClick={() => setIsAddModelModalOpen(true)}>
            <span>{intl.formatMessage(i18n.changeModel)}</span>
            <Sliders className="ml-auto h-4 w-4 rotate-90" />
          </DropdownMenuItem>
          {currentProvider === 'local' && currentModel && (
            <DropdownMenuItem onClick={() => setIsLocalModelSettingsOpen(true)}>
              <span>{intl.formatMessage(i18n.localModelSettings)}</span>
              <Settings className="ml-auto h-4 w-4" />
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {isAddModelModalOpen ? (
        <SwitchModelModal
          sessionId={sessionId}
          setView={setView}
          onClose={() => setIsAddModelModalOpen(false)}
          sessionModel={currentModel}
          sessionProvider={currentProvider}
          onModelSelected={(model, provider) => handleModelSelected(model, provider)}
        />
      ) : null}

      {isLocalModelSettingsOpen && currentModel && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="bg-background-primary border border-border-primary rounded-lg shadow-lg w-[480px] max-h-[80vh] flex flex-col">
            <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle">
              <h3 className="text-sm font-medium text-text-default">
                {intl.formatMessage(i18n.localModelSettingsTitle, { modelName: getModelDisplayName(currentModel) })}
              </h3>
              <button
                onClick={() => setIsLocalModelSettingsOpen(false)}
                className="text-text-muted hover:text-text-default text-lg leading-none"
              >
                ×
              </button>
            </div>
            <ScrollArea className="flex-1 px-4 py-3 overflow-y-auto max-h-[calc(80vh-52px)]">
              <ModelSettingsPanel modelId={currentModel} />
            </ScrollArea>
          </div>
        </div>
      )}
    </div>
  );
}
