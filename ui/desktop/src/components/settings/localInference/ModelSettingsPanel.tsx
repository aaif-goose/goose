import { useState, useEffect, useCallback } from 'react';
import { RotateCcw } from 'lucide-react';
import { Button } from '../../ui/button';
import { Switch } from '../../ui/switch';
import {
  getModelSettings,
  listBuiltinChatTemplates,
  updateModelSettings,
  type ChatTemplate,
  type ModelSettings,
  type SamplingConfig,
  type ToolCallingMode,
} from '../../../acp/local-inference';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  inferenceBackend: {
    id: 'modelSettingsPanel.inferenceBackend',
    defaultMessage: 'Inference backend',
  },
  inferenceBackendDescription: {
    id: 'modelSettingsPanel.inferenceBackendDescription',
    defaultMessage:
      'Automatic uses llama.cpp for GGUF and eredu for SafeTensors. Eredu checks checkpoint compatibility when loading.',
  },
  automaticBackend: {
    id: 'modelSettingsPanel.automaticBackend',
    defaultMessage: 'Automatic',
  },
  loadingSettings: {
    id: 'modelSettingsPanel.loadingSettings',
    defaultMessage: 'Loading settings...',
  },
  saving: {
    id: 'modelSettingsPanel.saving',
    defaultMessage: 'Saving...',
  },
  reset: {
    id: 'modelSettingsPanel.reset',
    defaultMessage: 'Reset',
  },
  resetToDefaults: {
    id: 'modelSettingsPanel.resetToDefaults',
    defaultMessage: 'Reset to defaults',
  },
  contextAndGeneration: {
    id: 'modelSettingsPanel.contextAndGeneration',
    defaultMessage: 'Context & Generation',
  },
  contextSize: {
    id: 'modelSettingsPanel.contextSize',
    defaultMessage: 'Context size',
  },
  contextSizeDescription: {
    id: 'modelSettingsPanel.contextSizeDescription',
    defaultMessage: 'Max context window (0 = model default)',
  },
  maxOutputTokens: {
    id: 'modelSettingsPanel.maxOutputTokens',
    defaultMessage: 'Max output tokens',
  },
  maxOutputTokensDescription: {
    id: 'modelSettingsPanel.maxOutputTokensDescription',
    defaultMessage: 'Cap on generated tokens',
  },
  samplingStrategy: {
    id: 'modelSettingsPanel.samplingStrategy',
    defaultMessage: 'Sampling Strategy',
  },
  temperature: {
    id: 'modelSettingsPanel.temperature',
    defaultMessage: 'Temperature',
  },
  topK: {
    id: 'modelSettingsPanel.topK',
    defaultMessage: 'Top K',
  },
  topP: {
    id: 'modelSettingsPanel.topP',
    defaultMessage: 'Top P',
  },
  minP: {
    id: 'modelSettingsPanel.minP',
    defaultMessage: 'Min P',
  },
  seed: {
    id: 'modelSettingsPanel.seed',
    defaultMessage: 'Seed',
  },
  tauTargetEntropy: {
    id: 'modelSettingsPanel.tauTargetEntropy',
    defaultMessage: 'Tau (target entropy)',
  },
  etaLearningRate: {
    id: 'modelSettingsPanel.etaLearningRate',
    defaultMessage: 'Eta (learning rate)',
  },
  repetitionPenalty: {
    id: 'modelSettingsPanel.repetitionPenalty',
    defaultMessage: 'Repetition Penalty',
  },
  repeatPenalty: {
    id: 'modelSettingsPanel.repeatPenalty',
    defaultMessage: 'Repeat penalty',
  },
  repeatPenaltyDescription: {
    id: 'modelSettingsPanel.repeatPenaltyDescription',
    defaultMessage: '1.0 = off',
  },
  repeatWindow: {
    id: 'modelSettingsPanel.repeatWindow',
    defaultMessage: 'Repeat window',
  },
  repeatWindowDescription: {
    id: 'modelSettingsPanel.repeatWindowDescription',
    defaultMessage: 'Tokens to look back',
  },
  frequencyPenalty: {
    id: 'modelSettingsPanel.frequencyPenalty',
    defaultMessage: 'Frequency penalty',
  },
  frequencyPenaltyDescription: {
    id: 'modelSettingsPanel.frequencyPenaltyDescription',
    defaultMessage: '0.0 = off',
  },
  presencePenalty: {
    id: 'modelSettingsPanel.presencePenalty',
    defaultMessage: 'Presence penalty',
  },
  presencePenaltyDescription: {
    id: 'modelSettingsPanel.presencePenaltyDescription',
    defaultMessage: '0.0 = off',
  },
  performance: {
    id: 'modelSettingsPanel.performance',
    defaultMessage: 'Performance',
  },
  batchSize: {
    id: 'modelSettingsPanel.batchSize',
    defaultMessage: 'Batch size',
  },
  batchSizeDescription: {
    id: 'modelSettingsPanel.batchSizeDescription',
    defaultMessage: 'Prompt processing batch',
  },
  gpuLayers: {
    id: 'modelSettingsPanel.gpuLayers',
    defaultMessage: 'GPU layers',
  },
  gpuLayersDescription: {
    id: 'modelSettingsPanel.gpuLayersDescription',
    defaultMessage: 'Layers to offload to GPU',
  },
  threads: {
    id: 'modelSettingsPanel.threads',
    defaultMessage: 'Threads',
  },
  threadsDescription: {
    id: 'modelSettingsPanel.threadsDescription',
    defaultMessage: 'CPU threads for generation',
  },
  lockModelInRam: {
    id: 'modelSettingsPanel.lockModelInRam',
    defaultMessage: 'Lock model in RAM (mlock)',
  },
  lockModelInRamDescription: {
    id: 'modelSettingsPanel.lockModelInRamDescription',
    defaultMessage: 'Prevent model from being swapped to disk',
  },
  flashAttention: {
    id: 'modelSettingsPanel.flashAttention',
    defaultMessage: 'Flash attention',
  },
  flashAttentionDescription: {
    id: 'modelSettingsPanel.flashAttentionDescription',
    defaultMessage: 'Enable flash attention optimization',
  },
  toolCalling: {
    id: 'modelSettingsPanel.toolCalling',
    defaultMessage: 'Tool calling',
  },
  toolCallingDescription: {
    id: 'modelSettingsPanel.toolCallingDescription',
    defaultMessage: 'Choose how local models select native or emulated tool calling',
  },
  toolCallingAuto: {
    id: 'modelSettingsPanel.toolCallingAuto',
    defaultMessage: 'Auto',
  },
  toolCallingForceNative: {
    id: 'modelSettingsPanel.toolCallingForceNative',
    defaultMessage: 'Force native',
  },
  toolCallingForceEmulated: {
    id: 'modelSettingsPanel.toolCallingForceEmulated',
    defaultMessage: 'Force emulated',
  },
  chatTemplate: {
    id: 'modelSettingsPanel.chatTemplate',
    defaultMessage: 'Chat template',
  },
  chatTemplateDescription: {
    id: 'modelSettingsPanel.chatTemplateDescription',
    defaultMessage: 'Use embedded GGUF metadata, a llama.cpp built-in template, or inline Jinja',
  },
  chatTemplateEmbedded: {
    id: 'modelSettingsPanel.chatTemplateEmbedded',
    defaultMessage: 'Embedded',
  },
  chatTemplateBuiltin: {
    id: 'modelSettingsPanel.chatTemplateBuiltin',
    defaultMessage: 'Built-in',
  },
  chatTemplateCustomInline: {
    id: 'modelSettingsPanel.chatTemplateCustomInline',
    defaultMessage: 'Custom inline',
  },
  builtinChatTemplate: {
    id: 'modelSettingsPanel.builtinChatTemplate',
    defaultMessage: 'Built-in template',
  },
  builtinChatTemplateDescription: {
    id: 'modelSettingsPanel.builtinChatTemplateDescription',
    defaultMessage: 'Select a llama.cpp built-in template name',
  },
  customChatTemplate: {
    id: 'modelSettingsPanel.customChatTemplate',
    defaultMessage: 'Custom chat template',
  },
  customChatTemplateDescription: {
    id: 'modelSettingsPanel.customChatTemplateDescription',
    defaultMessage: 'Paste the full Jinja chat template source',
  },
});

const DEFAULT_SETTINGS: ModelSettings = {
  backendId: null,
  contextSize: null,
  maxOutputTokens: null,
  draftModel: null,
  sampling: {
    type: 'Temperature',
    temperature: 0.8,
    topK: 40,
    topP: 0.95,
    minP: 0.05,
    seed: null,
  },
  repeatPenalty: 1.0,
  repeatLastN: 64,
  frequencyPenalty: 0.0,
  presencePenalty: 0.0,
  nBatch: null,
  nGpuLayers: null,
  useMlock: false,
  flashAttention: null,
  nThreads: null,
  toolCalling: 'auto',
  chatTemplate: { type: 'embedded' },
  enableThinking: true,
  visionCapable: false,
  imageTokenEstimate: 256,
  mmprojSizeBytes: 0,
};

type SamplingType = SamplingConfig['type'];
type ChatTemplateMode = 'embedded' | 'builtin' | 'custom_inline';

function NumberField({
  label,
  description,
  value,
  onChange,
  placeholder,
  min,
  max,
  step,
  allowNull,
}: {
  label: string;
  description?: string;
  value: number | null | undefined;
  onChange: (v: number | null) => void;
  placeholder?: string;
  min?: number;
  max?: number;
  step?: number;
  allowNull?: boolean;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs font-medium text-text-default">{label}</label>
      {description && <span className="text-xs text-text-muted">{description}</span>}
      <input
        type="number"
        className="w-full rounded border border-border-subtle bg-background-default px-2 py-1 text-sm text-text-default"
        value={value ?? ''}
        onChange={(e) => {
          const raw = e.target.value;
          if (raw === '' && allowNull) {
            onChange(null);
          } else {
            const n = step && step < 1 ? parseFloat(raw) : parseInt(raw, 10);
            if (!isNaN(n)) onChange(n);
          }
        }}
        placeholder={placeholder ?? 'Auto'}
        min={min}
        max={max}
        step={step}
      />
    </div>
  );
}

function ToggleField({
  label,
  description,
  value,
  onChange,
}: {
  label: string;
  description?: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <div>
        <div className="text-xs font-medium text-text-default">{label}</div>
        {description && <span className="text-xs text-text-muted">{description}</span>}
      </div>
      <Switch checked={value} onCheckedChange={onChange} variant="mono" />
    </div>
  );
}

function SelectField<T extends string>({
  label,
  description,
  value,
  options,
  onChange,
}: {
  label: string;
  description?: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <div>
        <div className="text-xs font-medium text-text-default">{label}</div>
        {description && <span className="text-xs text-text-muted">{description}</span>}
      </div>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        className="rounded border border-border-subtle bg-background-default px-2 py-1 text-xs text-text-default"
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}

function TextAreaField({
  label,
  description,
  value,
  onChange,
  onBlur,
}: {
  label: string;
  description?: string;
  value: string;
  onChange: (v: string) => void;
  onBlur: () => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs font-medium text-text-default">{label}</label>
      {description && <span className="text-xs text-text-muted">{description}</span>}
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        spellCheck={false}
        className="min-h-32 rounded border border-border-subtle bg-background-default px-2 py-1 font-mono text-xs text-text-default"
      />
    </div>
  );
}

export const ModelSettingsPanel = ({ modelId }: { modelId: string }) => {
  const intl = useIntl();
  const [settings, setSettings] = useState<ModelSettings>(DEFAULT_SETTINGS);
  const [chatTemplateDraft, setChatTemplateDraft] = useState('');
  const [builtinTemplateDraft, setBuiltinTemplateDraft] = useState('chatml');
  const [builtinTemplateOptions, setBuiltinTemplateOptions] = useState<string[]>(['chatml']);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [defaultBackend, setDefaultBackend] = useState('llamacpp');
  const [availableBackends, setAvailableBackends] = useState<string[]>(['llamacpp']);

  const load = useCallback(async () => {
    try {
      const [settingsResult, builtinsResult] = await Promise.allSettled([
        getModelSettings(modelId),
        listBuiltinChatTemplates(),
      ]);
      if (builtinsResult.status === 'fulfilled' && builtinsResult.value.length) {
        setBuiltinTemplateOptions(builtinsResult.value);
      }
      if (settingsResult.status === 'fulfilled') {
        const { settings: loaded, defaultBackend, availableBackends } = settingsResult.value;
        setDefaultBackend(defaultBackend || 'llamacpp');
        setAvailableBackends(availableBackends ?? ['llamacpp']);
        setSettings({
          ...loaded,
          toolCalling: loaded.toolCalling ?? 'auto',
          chatTemplate: loaded.chatTemplate ?? { type: 'embedded' },
        });
      }
    } catch {
      // use defaults
    } finally {
      setLoading(false);
    }
  }, [modelId]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    const chatTemplate = settings.chatTemplate;
    if (chatTemplate?.type === 'custom_inline') {
      setChatTemplateDraft(chatTemplate.template ?? '');
    } else {
      setChatTemplateDraft('');
    }
    if (chatTemplate?.type === 'builtin') {
      setBuiltinTemplateDraft(chatTemplate.name ?? 'chatml');
    }
  }, [settings.chatTemplate]);

  const save = async (updated: ModelSettings) => {
    const previous = settings;
    setSettings(updated);
    setSaveError(null);
    setSaving(true);
    try {
      setSettings(await updateModelSettings(modelId, updated));
    } catch (e) {
      setSettings(previous);
      setSaveError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const resetDefaults = () => save(DEFAULT_SETTINGS);

  const updateField = <K extends keyof ModelSettings>(key: K, value: ModelSettings[K]) => {
    save({ ...settings, [key]: value });
  };

  const effectiveBackend =
    settings.backendId === 'mlx' ? 'eredu' : settings.backendId || defaultBackend;
  const usesEredu = effectiveBackend === 'eredu';

  const samplingType: SamplingType = settings.sampling?.type ?? 'Temperature';
  const chatTemplate = settings.chatTemplate ?? { type: 'embedded' };
  const chatTemplateMode: ChatTemplateMode =
    chatTemplate.type === 'custom_inline'
      ? 'custom_inline'
      : chatTemplate.type === 'builtin'
        ? 'builtin'
        : 'embedded';

  const setChatTemplateMode = (mode: ChatTemplateMode) => {
    let next: ChatTemplate;
    if (mode === 'custom_inline') {
      next = { type: 'custom_inline', template: chatTemplateDraft };
    } else if (mode === 'builtin') {
      next = {
        type: 'builtin',
        name: builtinTemplateDraft.trim() || builtinTemplateOptions[0] || 'chatml',
      };
    } else {
      next = { type: 'embedded' };
    }
    updateField('chatTemplate', next);
  };

  const setBuiltinTemplateName = (name: string) => {
    setBuiltinTemplateDraft(name);
    if (chatTemplateMode === 'builtin') {
      updateField('chatTemplate', { type: 'builtin', name });
    }
  };

  const saveChatTemplateDraft = () => {
    if (chatTemplateMode === 'custom_inline') {
      updateField('chatTemplate', { type: 'custom_inline', template: chatTemplateDraft });
    }
  };

  const setSamplingType = (type: SamplingType) => {
    let sampling: SamplingConfig;
    if (type === 'Greedy') {
      sampling = { type: 'Greedy' };
    } else if (type === 'MirostatV2') {
      sampling = { type: 'MirostatV2', tau: 5.0, eta: 0.1, seed: null };
    } else {
      sampling = {
        type: 'Temperature',
        temperature: 0.8,
        topK: 40,
        topP: 0.95,
        minP: 0.05,
        seed: null,
      };
    }
    save({ ...settings, sampling });
  };

  const updateSampling = (partial: Partial<SamplingConfig>) => {
    save({ ...settings, sampling: { ...settings.sampling!, ...partial } as SamplingConfig });
  };

  const visibleBuiltinTemplateOptions = builtinTemplateOptions.includes(builtinTemplateDraft)
    ? builtinTemplateOptions
    : [builtinTemplateDraft, ...builtinTemplateOptions].filter(Boolean);

  if (loading) {
    return (
      <div className="py-2 text-xs text-text-muted">{intl.formatMessage(i18n.loadingSettings)}</div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-end">
        {saving && (
          <span className="text-xs text-text-muted mr-auto">{intl.formatMessage(i18n.saving)}</span>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={resetDefaults}
          title={intl.formatMessage(i18n.resetToDefaults)}
        >
          <RotateCcw className="w-3.5 h-3.5 mr-1" />
          <span className="text-xs">{intl.formatMessage(i18n.reset)}</span>
        </Button>
      </div>

      {saveError && (
        <p role="alert" className="text-xs text-red-500">
          {saveError}
        </p>
      )}
      <SelectField<string>
        label={intl.formatMessage(i18n.inferenceBackend)}
        description={intl.formatMessage(i18n.inferenceBackendDescription)}
        value={settings.backendId === 'mlx' ? 'eredu' : settings.backendId || 'auto'}
        options={[
          { value: 'auto', label: intl.formatMessage(i18n.automaticBackend) },
          ...Array.from(
            new Set([...availableBackends, ...(settings.backendId ? [effectiveBackend] : [])])
          ).map((backend) => ({
            value: backend,
            label: backend === 'llamacpp' ? 'llama.cpp' : 'eredu',
          })),
        ]}
        onChange={(backend) =>
          save({
            ...settings,
            backendId: backend === 'auto' ? null : backend,
            chatTemplate:
              (backend === 'eredu' || (backend === 'auto' && defaultBackend === 'eredu')) &&
              settings.chatTemplate?.type === 'builtin'
                ? { type: 'embedded' }
                : settings.chatTemplate,
          })
        }
      />

      {/* Context & Generation */}
      <div className="space-y-2">
        <h5 className="text-xs font-medium text-text-default">
          {intl.formatMessage(i18n.contextAndGeneration)}
        </h5>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label={intl.formatMessage(i18n.contextSize)}
            description={intl.formatMessage(i18n.contextSizeDescription)}
            value={settings.contextSize}
            onChange={(v) => updateField('contextSize', v)}
            placeholder="Auto"
            min={0}
            allowNull
          />
          <NumberField
            label={intl.formatMessage(i18n.maxOutputTokens)}
            description={intl.formatMessage(i18n.maxOutputTokensDescription)}
            value={settings.maxOutputTokens}
            onChange={(v) => updateField('maxOutputTokens', v)}
            placeholder="No limit"
            min={1}
            allowNull
          />
        </div>
      </div>

      {/* Sampling */}
      <div className="space-y-2">
        <SelectField
          label={intl.formatMessage(i18n.samplingStrategy)}
          value={samplingType}
          options={[
            { value: 'Greedy' as SamplingType, label: 'Greedy' },
            { value: 'Temperature' as SamplingType, label: 'Temperature' },
            { value: 'MirostatV2' as SamplingType, label: 'Mirostat v2' },
          ]}
          onChange={(v) => setSamplingType(v)}
        />

        {samplingType === 'Temperature' && settings.sampling?.type === 'Temperature' && (
          <div className="grid grid-cols-2 gap-3">
            <NumberField
              label={intl.formatMessage(i18n.temperature)}
              value={settings.sampling.temperature}
              onChange={(v) => updateSampling({ temperature: v ?? 0.8 })}
              min={0}
              max={2}
              step={0.05}
            />
            <NumberField
              label={intl.formatMessage(i18n.topK)}
              value={settings.sampling.topK}
              onChange={(v) => updateSampling({ topK: v ?? 40 })}
              min={0}
            />
            <NumberField
              label={intl.formatMessage(i18n.topP)}
              value={settings.sampling.topP}
              onChange={(v) => updateSampling({ topP: v ?? 0.95 })}
              min={0}
              max={1}
              step={0.01}
            />
            <NumberField
              label={intl.formatMessage(i18n.minP)}
              value={settings.sampling.minP}
              onChange={(v) => updateSampling({ minP: v ?? 0.05 })}
              min={0}
              max={1}
              step={0.01}
            />
            <NumberField
              label={intl.formatMessage(i18n.seed)}
              value={settings.sampling.seed}
              onChange={(v) => updateSampling({ seed: v })}
              placeholder="Random"
              min={0}
              allowNull
            />
          </div>
        )}

        {samplingType === 'MirostatV2' && settings.sampling?.type === 'MirostatV2' && (
          <div className="grid grid-cols-2 gap-3">
            <NumberField
              label={intl.formatMessage(i18n.tauTargetEntropy)}
              value={settings.sampling.tau}
              onChange={(v) => updateSampling({ tau: v ?? 5.0 })}
              min={0}
              step={0.1}
            />
            <NumberField
              label={intl.formatMessage(i18n.etaLearningRate)}
              value={settings.sampling.eta}
              onChange={(v) => updateSampling({ eta: v ?? 0.1 })}
              min={0}
              max={1}
              step={0.01}
            />
            <NumberField
              label={intl.formatMessage(i18n.seed)}
              value={settings.sampling.seed}
              onChange={(v) => updateSampling({ seed: v })}
              placeholder="Random"
              min={0}
              allowNull
            />
          </div>
        )}
      </div>

      {/* Repetition Penalty */}
      <div className="space-y-2">
        <h5 className="text-xs font-medium text-text-default">
          {intl.formatMessage(i18n.repetitionPenalty)}
        </h5>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label={intl.formatMessage(i18n.repeatPenalty)}
            description={intl.formatMessage(i18n.repeatPenaltyDescription)}
            value={settings.repeatPenalty}
            onChange={(v) => updateField('repeatPenalty', v ?? 1.0)}
            min={0}
            step={0.05}
          />
          <NumberField
            label={intl.formatMessage(i18n.repeatWindow)}
            description={intl.formatMessage(i18n.repeatWindowDescription)}
            value={settings.repeatLastN}
            onChange={(v) => updateField('repeatLastN', v ?? 64)}
            min={0}
          />
          <NumberField
            label={intl.formatMessage(i18n.frequencyPenalty)}
            description={intl.formatMessage(i18n.frequencyPenaltyDescription)}
            value={settings.frequencyPenalty}
            onChange={(v) => updateField('frequencyPenalty', v ?? 0.0)}
            min={0}
            max={2}
            step={0.05}
          />
          <NumberField
            label={intl.formatMessage(i18n.presencePenalty)}
            description={intl.formatMessage(i18n.presencePenaltyDescription)}
            value={settings.presencePenalty}
            onChange={(v) => updateField('presencePenalty', v ?? 0.0)}
            min={0}
            max={2}
            step={0.05}
          />
        </div>
      </div>

      {/* Performance */}
      <div className="space-y-2">
        {!usesEredu && (
          <>
            <h5 className="text-xs font-medium text-text-default">
              {intl.formatMessage(i18n.performance)}
            </h5>
            <div className="grid grid-cols-2 gap-3">
              <NumberField
                label={intl.formatMessage(i18n.batchSize)}
                description={intl.formatMessage(i18n.batchSizeDescription)}
                value={settings.nBatch}
                onChange={(v) => updateField('nBatch', v)}
                placeholder="Auto"
                min={1}
                allowNull
              />
              <NumberField
                label={intl.formatMessage(i18n.gpuLayers)}
                description={intl.formatMessage(i18n.gpuLayersDescription)}
                value={settings.nGpuLayers}
                onChange={(v) => updateField('nGpuLayers', v)}
                placeholder="All"
                min={0}
                allowNull
              />
              <NumberField
                label={intl.formatMessage(i18n.threads)}
                description={intl.formatMessage(i18n.threadsDescription)}
                value={settings.nThreads}
                onChange={(v) => updateField('nThreads', v)}
                placeholder="Auto"
                min={1}
                allowNull
              />
            </div>
            <ToggleField
              label={intl.formatMessage(i18n.lockModelInRam)}
              description={intl.formatMessage(i18n.lockModelInRamDescription)}
              value={settings.useMlock ?? false}
              onChange={(v) => updateField('useMlock', v)}
            />
            <SelectField
              label={intl.formatMessage(i18n.flashAttention)}
              description={intl.formatMessage(i18n.flashAttentionDescription)}
              value={
                settings.flashAttention === null || settings.flashAttention === undefined
                  ? 'auto'
                  : settings.flashAttention
                    ? 'on'
                    : 'off'
              }
              options={[
                { value: 'auto', label: 'Auto' },
                { value: 'on', label: 'On' },
                { value: 'off', label: 'Off' },
              ]}
              onChange={(v) => updateField('flashAttention', v === 'auto' ? null : v === 'on')}
            />
          </>
        )}
        <SelectField<ToolCallingMode>
          label={intl.formatMessage(i18n.toolCalling)}
          description={intl.formatMessage(i18n.toolCallingDescription)}
          value={settings.toolCalling ?? 'auto'}
          options={[
            { value: 'auto', label: intl.formatMessage(i18n.toolCallingAuto) },
            { value: 'force_native', label: intl.formatMessage(i18n.toolCallingForceNative) },
            { value: 'force_emulated', label: intl.formatMessage(i18n.toolCallingForceEmulated) },
          ]}
          onChange={(v) => updateField('toolCalling', v)}
        />
        <SelectField<ChatTemplateMode>
          label={intl.formatMessage(i18n.chatTemplate)}
          description={intl.formatMessage(i18n.chatTemplateDescription)}
          value={chatTemplateMode}
          options={[
            { value: 'embedded', label: intl.formatMessage(i18n.chatTemplateEmbedded) },
            ...(!usesEredu
              ? [
                  {
                    value: 'builtin' as ChatTemplateMode,
                    label: intl.formatMessage(i18n.chatTemplateBuiltin),
                  },
                ]
              : []),
            {
              value: 'custom_inline',
              label: intl.formatMessage(i18n.chatTemplateCustomInline),
            },
          ]}
          onChange={setChatTemplateMode}
        />
        {chatTemplateMode === 'builtin' && (
          <SelectField<string>
            label={intl.formatMessage(i18n.builtinChatTemplate)}
            description={intl.formatMessage(i18n.builtinChatTemplateDescription)}
            value={builtinTemplateDraft}
            options={visibleBuiltinTemplateOptions.map((template) => ({
              value: template,
              label: template,
            }))}
            onChange={setBuiltinTemplateName}
          />
        )}
        {chatTemplateMode === 'custom_inline' && (
          <TextAreaField
            label={intl.formatMessage(i18n.customChatTemplate)}
            description={intl.formatMessage(i18n.customChatTemplateDescription)}
            value={chatTemplateDraft}
            onChange={setChatTemplateDraft}
            onBlur={saveChatTemplateDraft}
          />
        )}
      </div>
    </div>
  );
};
