import { useState, useEffect, useMemo } from 'react';
import { Switch } from '../../ui/switch';
import { useConfig } from '../../ConfigContext';
import { trackSettingToggled } from '../../../utils/analytics';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  enablePromptInjection: {
    id: 'securityToggle.enablePromptInjection',
    defaultMessage: 'Enable Prompt Injection Detection',
  },
  promptInjectionDescription: {
    id: 'securityToggle.promptInjectionDescription',
    defaultMessage: 'Detect and prevent potential prompt injection attacks',
  },
  detectionThreshold: {
    id: 'securityToggle.detectionThreshold',
    defaultMessage: 'Detection Threshold',
  },
  thresholdDescription: {
    id: 'securityToggle.thresholdDescription',
    defaultMessage: 'Higher values are more strict (0.01 = very lenient, 1.0 = maximum strict)',
  },
  enableCommandInjection: {
    id: 'securityToggle.enableCommandInjection',
    defaultMessage: 'Enable Command Injection ML Detection',
  },
  commandInjectionDescription: {
    id: 'securityToggle.commandInjectionDescription',
    defaultMessage: 'Use ML models to detect malicious shell commands',
  },
  commandClassifierActive: {
    id: 'securityToggle.commandClassifierActive',
    defaultMessage: 'Command classifier active (auto-configured from environment)',
  },
  enablePromptInjectionMl: {
    id: 'securityToggle.enablePromptInjectionMl',
    defaultMessage: 'Enable Prompt Injection ML Detection',
  },
  promptInjectionMlDescription: {
    id: 'securityToggle.promptInjectionMlDescription',
    defaultMessage: 'Use ML models to detect potential prompt injection in your chat',
  },
  detectionModel: {
    id: 'securityToggle.detectionModel',
    defaultMessage: 'Detection Model',
  },
  detectionModelDescription: {
    id: 'securityToggle.detectionModelDescription',
    defaultMessage: 'Select which ML model to use for prompt injection detection',
  },
  classificationEndpoint: {
    id: 'securityToggle.classificationEndpoint',
    defaultMessage: 'Classification Endpoint',
  },
  classificationEndpointDescription: {
    id: 'securityToggle.classificationEndpointDescription',
    defaultMessage: 'Enter the full URL for your classification service',
  },
  apiTokenOptional: {
    id: 'securityToggle.apiTokenOptional',
    defaultMessage: 'API Token (Optional)',
  },
  apiTokenDescription: {
    id: 'securityToggle.apiTokenDescription',
    defaultMessage: 'Authentication token for the classification service',
  },
  commandEndpointDescription: {
    id: 'securityToggle.commandEndpointDescription',
    defaultMessage: 'Enter the full URL for your command injection classification service',
  },
  mlEndpointDescription: {
    id: 'securityToggle.mlEndpointDescription',
    defaultMessage: 'Enter the full URL for your ML classification service (including model identifier)',
  },
  mlTokenDescription: {
    id: 'securityToggle.mlTokenDescription',
    defaultMessage: 'Authentication token for the ML service (e.g., HuggingFace token)',
  },
});

interface SecurityConfig {
  SECURITY_PROMPT_ENABLED?: boolean;
  SECURITY_PROMPT_THRESHOLD?: number;
  SECURITY_PROMPT_CLASSIFIER_ENABLED?: boolean;
  SECURITY_PROMPT_CLASSIFIER_MODEL?: string;
  SECURITY_PROMPT_CLASSIFIER_ENDPOINT?: string;
  SECURITY_PROMPT_CLASSIFIER_TOKEN?: string;
  SECURITY_COMMAND_CLASSIFIER_ENABLED?: boolean;
  SECURITY_COMMAND_CLASSIFIER_ENDPOINT?: string;
  SECURITY_COMMAND_CLASSIFIER_TOKEN?: string;
}

interface ClassifierEndpointInputsProps {
  endpointValue: string;
  tokenValue: string;
  onEndpointChange: (value: string) => void;
  onTokenChange: (value: string) => void;
  onEndpointBlur: (value: string) => void;
  onTokenBlur: (value: string) => void;
  disabled: boolean;
  endpointPlaceholder: string;
  tokenPlaceholder: string;
  endpointLabel?: string;
  endpointDescription?: string;
  tokenLabel?: string;
  tokenDescription?: string;
}

const ClassifierEndpointInputs = ({
  endpointValue,
  tokenValue,
  onEndpointChange,
  onTokenChange,
  onEndpointBlur,
  onTokenBlur,
  disabled,
  endpointPlaceholder,
  tokenPlaceholder,
  endpointLabel,
  endpointDescription,
  tokenLabel,
  tokenDescription,
}: ClassifierEndpointInputsProps) => {
  return (
    <div className="space-y-3">
      <div>
        <label
          className={`text-sm font-medium ${disabled ? 'text-text-secondary' : 'text-text-primary'}`}
        >
          {endpointLabel}
        </label>
        <p className="text-xs text-text-secondary mb-2">{endpointDescription}</p>
        <input
          type="url"
          value={endpointValue}
          onChange={(e) => onEndpointChange(e.target.value)}
          onBlur={(e) => onEndpointBlur(e.target.value)}
          disabled={disabled}
          placeholder={endpointPlaceholder}
          className={`w-full px-3 py-2 text-sm border rounded placeholder:text-text-secondary ${
            disabled
              ? 'border-border-primary bg-background-secondary text-text-secondary cursor-not-allowed'
              : 'border-border-primary bg-background-primary text-text-primary'
          }`}
        />
      </div>

      <div>
        <label
          className={`text-sm font-medium ${disabled ? 'text-text-secondary' : 'text-text-primary'}`}
        >
          {tokenLabel}
        </label>
        <p className="text-xs text-text-secondary mb-2">{tokenDescription}</p>
        <input
          type="password"
          value={tokenValue}
          onChange={(e) => onTokenChange(e.target.value)}
          onBlur={(e) => onTokenBlur(e.target.value)}
          disabled={disabled}
          placeholder={tokenPlaceholder}
          className={`w-full px-3 py-2 text-sm border rounded placeholder:text-text-secondary ${
            disabled
              ? 'border-border-primary bg-background-secondary text-text-secondary cursor-not-allowed'
              : 'border-border-primary bg-background-primary text-text-primary'
          }`}
        />
      </div>
    </div>
  );
};

export const SecurityToggle = () => {
  const intl = useIntl();
  const { config, upsert } = useConfig();

  const modelMapping = useMemo(() => {
    const mappingEnv = window.appConfig?.get('SECURITY_ML_MODEL_MAPPING') as string | undefined;
    if (!mappingEnv) {
      return null;
    }

    try {
      return JSON.parse(mappingEnv) as Record<string, { model_type?: string }>;
    } catch {
      return null;
    }
  }, []);

  const availablePromptModels = useMemo(() => {
    if (!modelMapping) {
      return [];
    }

    return Object.entries(modelMapping)
      .filter(([_, modelInfo]) => modelInfo.model_type === 'prompt')
      .map(([modelName, _]) => ({
        value: modelName,
        label: modelName,
      }));
  }, [modelMapping]);

  const showModelDropdown = useMemo(() => {
    return availablePromptModels.length > 0;
  }, [availablePromptModels]);

  const {
    SECURITY_PROMPT_ENABLED: enabled = false,
    SECURITY_PROMPT_THRESHOLD: configThreshold = 0.8,
    SECURITY_PROMPT_CLASSIFIER_ENABLED: mlEnabled = false,
    SECURITY_PROMPT_CLASSIFIER_MODEL: mlModel = '',
    SECURITY_PROMPT_CLASSIFIER_ENDPOINT: mlEndpoint = '',
    SECURITY_PROMPT_CLASSIFIER_TOKEN: mlToken = '',
    SECURITY_COMMAND_CLASSIFIER_ENABLED: commandClassifierEnabled,
    SECURITY_COMMAND_CLASSIFIER_ENDPOINT: commandEndpoint = '',
    SECURITY_COMMAND_CLASSIFIER_TOKEN: commandToken = '',
  } = (config as SecurityConfig) ?? {};

  const hasCommandModel = useMemo(() => {
    if (!modelMapping) {
      return false;
    }
    return Object.values(modelMapping).some((modelInfo) => modelInfo.model_type === 'command');
  }, [modelMapping]);

  const effectiveCommandClassifierEnabled = commandClassifierEnabled ?? false;
  const effectiveModel = mlModel || availablePromptModels[0]?.value || '';
  const [thresholdInput, setThresholdInput] = useState(configThreshold.toString());
  const [endpointInput, setEndpointInput] = useState(mlEndpoint);
  const [tokenInput, setTokenInput] = useState(mlToken);
  const [commandEndpointInput, setCommandEndpointInput] = useState(commandEndpoint);
  const [commandTokenInput, setCommandTokenInput] = useState(commandToken);

  useEffect(() => {
    setThresholdInput(configThreshold.toString());
    setEndpointInput(mlEndpoint);
    setTokenInput(mlToken);
    setCommandEndpointInput(commandEndpoint);
    setCommandTokenInput(commandToken);
  }, [configThreshold, mlEndpoint, mlToken, commandEndpoint, commandToken]);

  const handleToggle = async (enabled: boolean) => {
    await upsert('SECURITY_PROMPT_ENABLED', enabled, false);
    trackSettingToggled('prompt_injection_detection', enabled);
  };

  const handleThresholdChange = async (threshold: number) => {
    const validThreshold = Math.max(0, Math.min(1, threshold));
    await upsert('SECURITY_PROMPT_THRESHOLD', validThreshold, false);
  };

  const handleMlToggle = async (enabled: boolean) => {
    await upsert('SECURITY_PROMPT_CLASSIFIER_ENABLED', enabled, false);

    if (enabled) {
      if (showModelDropdown) {
        const modelToSet = mlModel || availablePromptModels[0]?.value;
        if (modelToSet) {
          await upsert('SECURITY_PROMPT_CLASSIFIER_MODEL', modelToSet, false);
        }
      } else {
        await upsert('SECURITY_PROMPT_CLASSIFIER_MODEL', '', false);
      }
    }
  };

  const handleModelChange = async (model: string) => {
    await upsert('SECURITY_PROMPT_CLASSIFIER_MODEL', model, false);
  };

  const handleEndpointChange = async (endpoint: string) => {
    await upsert('SECURITY_PROMPT_CLASSIFIER_ENDPOINT', endpoint, false);
  };

  const handleTokenChange = async (token: string) => {
    await upsert('SECURITY_PROMPT_CLASSIFIER_TOKEN', token, true); // true = secret
  };

  const handleCommandClassifierToggle = async (enabled: boolean) => {
    await upsert('SECURITY_COMMAND_CLASSIFIER_ENABLED', enabled, false);
    trackSettingToggled('command_classifier', enabled);
  };

  const handleCommandEndpointChange = async (endpoint: string) => {
    await upsert('SECURITY_COMMAND_CLASSIFIER_ENDPOINT', endpoint, false);
  };

  const handleCommandTokenChange = async (token: string) => {
    await upsert('SECURITY_COMMAND_CLASSIFIER_TOKEN', token, true); // true = secret
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between py-2 px-2 hover:bg-background-secondary rounded-lg transition-all">
        <div>
          <h3 className="text-text-primary">{intl.formatMessage(i18n.enablePromptInjection)}</h3>
          <p className="text-xs text-text-secondary max-w-md mt-[2px]">
            {intl.formatMessage(i18n.promptInjectionDescription)}
          </p>
        </div>
        <div className="flex items-center">
          <Switch checked={enabled} onCheckedChange={handleToggle} variant="mono" />
        </div>
      </div>

      <div
        className={`overflow-hidden transition-all duration-300 ease-in-out ${
          enabled ? 'max-h-[1000px] opacity-100' : 'max-h-0 opacity-0'
        }`}
      >
        <div className="space-y-4 px-2 pb-2">
          {/* Detection Threshold */}
          <div className={enabled ? '' : 'opacity-50'}>
            <label
              className={`text-sm font-medium ${enabled ? 'text-text-primary' : 'text-text-secondary'}`}
            >
              {intl.formatMessage(i18n.detectionThreshold)}
            </label>
            <p className="text-xs text-text-secondary mb-2">
              {intl.formatMessage(i18n.thresholdDescription)}
            </p>
            <input
              type="number"
              min={0.01}
              max={1.0}
              step={0.01}
              value={thresholdInput}
              onChange={(e) => {
                setThresholdInput(e.target.value);
              }}
              onBlur={(e) => {
                const value = parseFloat(e.target.value);
                if (isNaN(value) || value < 0.01 || value > 1.0) {
                  // Revert to previous valid value
                  setThresholdInput(configThreshold.toString());
                } else {
                  handleThresholdChange(value);
                }
              }}
              disabled={!enabled}
              className={`w-24 px-2 py-1 text-sm border rounded ${
                enabled
                  ? 'border-border-primary bg-background-primary text-text-primary'
                  : 'border-border-primary bg-background-secondary text-text-secondary cursor-not-allowed'
              }`}
              placeholder="0.80"
            />
          </div>

          {/* Command Injection Detection Toggle */}
          <div className="border-t border-border-primary pt-4">
            <div className="flex items-center justify-between py-2 hover:bg-background-secondary rounded-lg transition-all">
              <div>
                <h4
                  className={`text-sm font-medium ${enabled ? 'text-text-primary' : 'text-text-secondary'}`}
                >
                  {intl.formatMessage(i18n.enableCommandInjection)}
                </h4>
                <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                  {intl.formatMessage(i18n.commandInjectionDescription)}
                </p>
              </div>
              <div className="flex items-center">
                <Switch
                  checked={effectiveCommandClassifierEnabled}
                  onCheckedChange={handleCommandClassifierToggle}
                  disabled={!enabled}
                  variant="mono"
                />
              </div>
            </div>

            {hasCommandModel ? (
              enabled &&
              effectiveCommandClassifierEnabled && (
                <div className="text-sm text-gray-700 dark:text-gray-300 mt-2">
                  ✓ {intl.formatMessage(i18n.commandClassifierActive)}
                </div>
              )
            ) : (
              <div
                className={`overflow-hidden transition-all duration-300 ease-in-out ${
                  enabled && effectiveCommandClassifierEnabled
                    ? 'max-h-[32rem] opacity-100 mt-3'
                    : 'max-h-0 opacity-0'
                }`}
              >
                <div className={enabled && effectiveCommandClassifierEnabled ? '' : 'opacity-50'}>
                  <ClassifierEndpointInputs
                    endpointValue={commandEndpointInput}
                    tokenValue={commandTokenInput}
                    onEndpointChange={setCommandEndpointInput}
                    onTokenChange={setCommandTokenInput}
                    onEndpointBlur={handleCommandEndpointChange}
                    onTokenBlur={handleCommandTokenChange}
                    disabled={!enabled || !effectiveCommandClassifierEnabled}
                    endpointPlaceholder="https://example.com/classify"
                    tokenPlaceholder="token..."
                    endpointLabel={intl.formatMessage(i18n.classificationEndpoint)}
                    endpointDescription={intl.formatMessage(i18n.commandEndpointDescription)}
                    tokenLabel={intl.formatMessage(i18n.apiTokenOptional)}
                    tokenDescription={intl.formatMessage(i18n.apiTokenDescription)}
                  />
                </div>
              </div>
            )}
          </div>

          {/* Prompt Injection Detection Toggle */}
          <div className="border-t border-border-primary pt-4">
            <div className="flex items-center justify-between py-2 hover:bg-background-secondary rounded-lg transition-all">
              <div>
                <h4
                  className={`text-sm font-medium ${enabled ? 'text-text-primary' : 'text-text-secondary'}`}
                >
                  {intl.formatMessage(i18n.enablePromptInjectionMl)}
                </h4>
                <p className="text-xs text-text-secondary max-w-md mt-[2px]">
                  {intl.formatMessage(i18n.promptInjectionMlDescription)}
                </p>
              </div>
              <div className="flex items-center">
                <Switch
                  checked={mlEnabled}
                  onCheckedChange={handleMlToggle}
                  disabled={!enabled}
                  variant="mono"
                />
              </div>
            </div>

            {/* Configuration Section */}
            <div
              className={`overflow-hidden transition-all duration-300 ease-in-out ${
                enabled && mlEnabled ? 'max-h-[32rem] opacity-100 mt-3' : 'max-h-0 opacity-0'
              }`}
            >
              <div className={enabled && mlEnabled ? '' : 'opacity-50'}>
                {showModelDropdown ? (
                  <div className="space-y-3">
                    <div>
                      <label
                        className={`text-sm font-medium ${enabled && mlEnabled ? 'text-text-primary' : 'text-text-secondary'}`}
                      >
                        {intl.formatMessage(i18n.detectionModel)}
                      </label>
                      <p className="text-xs text-text-secondary mb-2">
                        {intl.formatMessage(i18n.detectionModelDescription)}
                      </p>
                      <select
                        value={effectiveModel}
                        onChange={(e) => handleModelChange(e.target.value)}
                        disabled={!enabled || !mlEnabled}
                        className={`w-full px-3 py-2 text-sm border rounded ${
                          enabled && mlEnabled
                            ? 'border-border-primary bg-background-primary text-text-primary'
                            : 'border-border-primary bg-background-secondary text-text-secondary cursor-not-allowed'
                        }`}
                      >
                        {availablePromptModels.map((model) => (
                          <option key={model.value} value={model.value}>
                            {model.label}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                ) : (
                  <ClassifierEndpointInputs
                    endpointValue={endpointInput}
                    tokenValue={tokenInput}
                    onEndpointChange={setEndpointInput}
                    onTokenChange={setTokenInput}
                    onEndpointBlur={handleEndpointChange}
                    onTokenBlur={handleTokenChange}
                    disabled={!enabled || !mlEnabled}
                    endpointPlaceholder="https://router.huggingface.co/hf-inference/models/protectai/deberta-v3-base-prompt-injection-v2"
                    tokenPlaceholder="hf_..."
                    endpointLabel={intl.formatMessage(i18n.classificationEndpoint)}
                    endpointDescription={intl.formatMessage(i18n.mlEndpointDescription)}
                    tokenLabel={intl.formatMessage(i18n.apiTokenOptional)}
                    tokenDescription={intl.formatMessage(i18n.mlTokenDescription)}
                  />
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
