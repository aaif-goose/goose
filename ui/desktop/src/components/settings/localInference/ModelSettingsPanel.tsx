import { useState, useEffect, useCallback } from 'react';
import { RotateCcw } from 'lucide-react';
import { Button } from '../../ui/button';
import { Switch } from '../../ui/switch';
import {
  getModelSettings,
  updateModelSettings,
  type ModelSettings,
  type SamplingConfig,
} from '../../../api';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
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
    defaultMessage: 'Tool Calling',
  },
  nativeToolCalling: {
    id: 'modelSettingsPanel.nativeToolCalling',
    defaultMessage: 'Native tool calling',
  },
  nativeToolCallingDescription: {
    id: 'modelSettingsPanel.nativeToolCallingDescription',
    defaultMessage:
      "Use the model's built-in tool-call format instead of the shell-command emulator. Enable for large models that reliably support tool calling.",
  },
});

const DEFAULT_SETTINGS: ModelSettings = {
  context_size: null,
  max_output_tokens: null,
  sampling: {
    type: 'Temperature',
    temperature: 0.8,
    top_k: 40,
    top_p: 0.95,
    min_p: 0.05,
    seed: null,
  },
  repeat_penalty: 1.0,
  repeat_last_n: 64,
  frequency_penalty: 0.0,
  presence_penalty: 0.0,
  n_batch: null,
  n_gpu_layers: null,
  use_mlock: false,
  flash_attention: null,
  n_threads: null,
  native_tool_calling: false,
};

type SamplingType = SamplingConfig['type'];

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

export const ModelSettingsPanel = ({ modelId }: { modelId: string }) => {
  const intl = useIntl();
  const [settings, setSettings] = useState<ModelSettings>(DEFAULT_SETTINGS);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    try {
      const res = await getModelSettings({ path: { model_id: modelId } });
      if (res.data) setSettings(res.data);
    } catch {
      // use defaults
    } finally {
      setLoading(false);
    }
  }, [modelId]);

  useEffect(() => {
    load();
  }, [load]);

  const save = async (updated: ModelSettings) => {
    setSettings(updated);
    setSaving(true);
    try {
      await updateModelSettings({ path: { model_id: modelId }, body: updated });
    } catch (e) {
      console.error('Failed to save settings:', e);
    } finally {
      setSaving(false);
    }
  };

  const resetDefaults = () => save(DEFAULT_SETTINGS);

  const updateField = <K extends keyof ModelSettings>(key: K, value: ModelSettings[K]) => {
    save({ ...settings, [key]: value });
  };

  const samplingType: SamplingType = settings.sampling?.type ?? 'Temperature';

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
        top_k: 40,
        top_p: 0.95,
        min_p: 0.05,
        seed: null,
      };
    }
    save({ ...settings, sampling });
  };

  const updateSampling = (partial: Partial<SamplingConfig>) => {
    save({ ...settings, sampling: { ...settings.sampling!, ...partial } as SamplingConfig });
  };

  if (loading) {
    return <div className="py-2 text-xs text-text-muted">{intl.formatMessage(i18n.loadingSettings)}</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-end">
        {saving && <span className="text-xs text-text-muted mr-auto">{intl.formatMessage(i18n.saving)}</span>}
        <Button variant="ghost" size="sm" onClick={resetDefaults} title={intl.formatMessage(i18n.resetToDefaults)}>
          <RotateCcw className="w-3.5 h-3.5 mr-1" />
          <span className="text-xs">{intl.formatMessage(i18n.reset)}</span>
        </Button>
      </div>

      {/* Context & Generation */}
      <div className="space-y-2">
        <h5 className="text-xs font-medium text-text-default">{intl.formatMessage(i18n.contextAndGeneration)}</h5>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label={intl.formatMessage(i18n.contextSize)}
            description={intl.formatMessage(i18n.contextSizeDescription)}
            value={settings.context_size}
            onChange={(v) => updateField('context_size', v)}
            placeholder="Auto"
            min={0}
            allowNull
          />
          <NumberField
            label={intl.formatMessage(i18n.maxOutputTokens)}
            description={intl.formatMessage(i18n.maxOutputTokensDescription)}
            value={settings.max_output_tokens}
            onChange={(v) => updateField('max_output_tokens', v)}
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
              value={settings.sampling.top_k}
              onChange={(v) => updateSampling({ top_k: v ?? 40 })}
              min={0}
            />
            <NumberField
              label={intl.formatMessage(i18n.topP)}
              value={settings.sampling.top_p}
              onChange={(v) => updateSampling({ top_p: v ?? 0.95 })}
              min={0}
              max={1}
              step={0.01}
            />
            <NumberField
              label={intl.formatMessage(i18n.minP)}
              value={settings.sampling.min_p}
              onChange={(v) => updateSampling({ min_p: v ?? 0.05 })}
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
        <h5 className="text-xs font-medium text-text-default">{intl.formatMessage(i18n.repetitionPenalty)}</h5>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label={intl.formatMessage(i18n.repeatPenalty)}
            description={intl.formatMessage(i18n.repeatPenaltyDescription)}
            value={settings.repeat_penalty}
            onChange={(v) => updateField('repeat_penalty', v ?? 1.0)}
            min={0}
            step={0.05}
          />
          <NumberField
            label={intl.formatMessage(i18n.repeatWindow)}
            description={intl.formatMessage(i18n.repeatWindowDescription)}
            value={settings.repeat_last_n}
            onChange={(v) => updateField('repeat_last_n', v ?? 64)}
            min={0}
          />
          <NumberField
            label={intl.formatMessage(i18n.frequencyPenalty)}
            description={intl.formatMessage(i18n.frequencyPenaltyDescription)}
            value={settings.frequency_penalty}
            onChange={(v) => updateField('frequency_penalty', v ?? 0.0)}
            min={0}
            max={2}
            step={0.05}
          />
          <NumberField
            label={intl.formatMessage(i18n.presencePenalty)}
            description={intl.formatMessage(i18n.presencePenaltyDescription)}
            value={settings.presence_penalty}
            onChange={(v) => updateField('presence_penalty', v ?? 0.0)}
            min={0}
            max={2}
            step={0.05}
          />
        </div>
      </div>

      {/* Performance */}
      <div className="space-y-2">
        <h5 className="text-xs font-medium text-text-default">{intl.formatMessage(i18n.performance)}</h5>
        <div className="grid grid-cols-2 gap-3">
          <NumberField
            label={intl.formatMessage(i18n.batchSize)}
            description={intl.formatMessage(i18n.batchSizeDescription)}
            value={settings.n_batch}
            onChange={(v) => updateField('n_batch', v)}
            placeholder="Auto"
            min={1}
            allowNull
          />
          <NumberField
            label={intl.formatMessage(i18n.gpuLayers)}
            description={intl.formatMessage(i18n.gpuLayersDescription)}
            value={settings.n_gpu_layers}
            onChange={(v) => updateField('n_gpu_layers', v)}
            placeholder="All"
            min={0}
            allowNull
          />
          <NumberField
            label={intl.formatMessage(i18n.threads)}
            description={intl.formatMessage(i18n.threadsDescription)}
            value={settings.n_threads}
            onChange={(v) => updateField('n_threads', v)}
            placeholder="Auto"
            min={1}
            allowNull
          />
        </div>
        <ToggleField
          label={intl.formatMessage(i18n.lockModelInRam)}
          description={intl.formatMessage(i18n.lockModelInRamDescription)}
          value={settings.use_mlock ?? false}
          onChange={(v) => updateField('use_mlock', v)}
        />
        <SelectField
          label={intl.formatMessage(i18n.flashAttention)}
          description={intl.formatMessage(i18n.flashAttentionDescription)}
          value={
            settings.flash_attention === null || settings.flash_attention === undefined
              ? 'auto'
              : settings.flash_attention
                ? 'on'
                : 'off'
          }
          options={[
            { value: 'auto', label: 'Auto' },
            { value: 'on', label: 'On' },
            { value: 'off', label: 'Off' },
          ]}
          onChange={(v) => updateField('flash_attention', v === 'auto' ? null : v === 'on')}
        />
      </div>
      {/* Tool Calling */}
      <div className="space-y-2">
        <h5 className="text-xs font-medium text-text-default">{intl.formatMessage(i18n.toolCalling)}</h5>
        <ToggleField
          label={intl.formatMessage(i18n.nativeToolCalling)}
          description={intl.formatMessage(i18n.nativeToolCallingDescription)}
          value={settings.native_tool_calling ?? false}
          onChange={(v) => updateField('native_tool_calling', v)}
        />
      </div>
    </div>
  );
};
