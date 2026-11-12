import { type ReactNode, useEffect, useRef, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { defineMessages, useIntl } from '../../i18n';
import { hasConfiguredSecret } from '../../acp/config';

const i18n = defineMessages({
  title: {
    id: 'settings.agentLoop.title',
    defaultMessage: 'Agent Loop',
  },
  operationsHeading: {
    id: 'settings.agentLoop.operations.heading',
    defaultMessage: 'Configurable operations',
  },
  enableOperation: {
    id: 'settings.agentLoop.operations.enable',
    defaultMessage: 'Enable {operation}',
  },
  slashCommandsTitle: {
    id: 'settings.agentLoop.operations.slashCommands.title',
    defaultMessage: 'Slash commands',
  },
  slashCommandsDescription: {
    id: 'settings.agentLoop.operations.slashCommands.description',
    defaultMessage: 'Recognizes commands such as /compact, /skills, and recipe shortcuts.',
  },
  autoEffortTitle: {
    id: 'settings.agentLoop.operations.autoEffort.title',
    defaultMessage: 'Automatic thinking effort',
  },
  autoEffortDescription: {
    id: 'settings.agentLoop.operations.autoEffort.description',
    defaultMessage:
      'Uses Jev to choose the thinking effort for each request. Request text is sent to TypeSafe AI; skill and recipe slash commands include their expanded local instructions.',
  },
  typesafeApiKeyLabel: {
    id: 'settings.agentLoop.operations.autoEffort.apiKey.label',
    defaultMessage: 'TypeSafe API key',
  },
  typesafeApiKeyPlaceholder: {
    id: 'settings.agentLoop.operations.autoEffort.apiKey.placeholder',
    defaultMessage: 'Enter API key',
  },
  apiKeyConfigured: {
    id: 'settings.agentLoop.operations.autoEffort.apiKey.configured',
    defaultMessage: 'Configured',
  },
  saveApiKey: {
    id: 'settings.agentLoop.operations.autoEffort.apiKey.save',
    defaultMessage: 'Save key',
  },
  removeApiKey: {
    id: 'settings.agentLoop.operations.autoEffort.apiKey.remove',
    defaultMessage: 'Remove key',
  },
  maxTurnsTitle: {
    id: 'settings.agentLoop.operations.maxTurns.title',
    defaultMessage: 'Max turns',
  },
  maxTurnsDescription: {
    id: 'settings.agentLoop.operations.maxTurns.description',
    defaultMessage: 'Stops autonomous work and asks the user before continuing.',
  },
  contextCompactionTitle: {
    id: 'settings.agentLoop.operations.contextCompaction.title',
    defaultMessage: 'Context compaction',
  },
  contextCompactionDescription: {
    id: 'settings.agentLoop.operations.contextCompaction.description',
    defaultMessage: 'Summarizes conversation history before the context window fills.',
  },
  toolPairCompactionTitle: {
    id: 'settings.agentLoop.operations.toolPairCompaction.title',
    defaultMessage: 'Tool pair compaction',
  },
  toolPairCompactionDescription: {
    id: 'settings.agentLoop.operations.toolPairCompaction.description',
    defaultMessage:
      'Summarizes older tool requests and responses to save context. The cutoff is automatic when blank.',
  },
  recipeRetryTitle: {
    id: 'settings.agentLoop.operations.recipeRetry.title',
    defaultMessage: 'Recipe retry',
  },
  recipeRetryDescription: {
    id: 'settings.agentLoop.operations.recipeRetry.description',
    defaultMessage: 'Runs recipe checks again when a configured success check fails.',
  },
  stopHooksTitle: {
    id: 'settings.agentLoop.operations.stopHooks.title',
    defaultMessage: 'Stop hooks',
  },
  stopHooksDescription: {
    id: 'settings.agentLoop.operations.stopHooks.description',
    defaultMessage: 'Lets hooks block completion while preventing endless stop cycles.',
  },
  turnsLabel: {
    id: 'settings.agentLoop.operations.turns.label',
    defaultMessage: 'Turns',
  },
  thresholdLabel: {
    id: 'settings.agentLoop.operations.threshold.label',
    defaultMessage: 'Threshold %',
  },
  tokenLimitLabel: {
    id: 'settings.agentLoop.operations.tokenLimit.label',
    defaultMessage: 'Token limit',
  },
  cutoffLabel: {
    id: 'settings.agentLoop.operations.cutoff.label',
    defaultMessage: 'Cutoff',
  },
  retryTimeoutLabel: {
    id: 'settings.agentLoop.operations.retryTimeout.label',
    defaultMessage: 'Retry timeout',
  },
  failureTimeoutLabel: {
    id: 'settings.agentLoop.operations.failureTimeout.label',
    defaultMessage: 'Failure timeout',
  },
  blockLimitLabel: {
    id: 'settings.agentLoop.operations.blockLimit.label',
    defaultMessage: 'Block limit',
  },
  autoPlaceholder: {
    id: 'settings.agentLoop.operations.auto.placeholder',
    defaultMessage: 'Auto',
  },
});

type NumberSetting =
  | 'maxTurns'
  | 'compactionThreshold'
  | 'compactionTokenLimit'
  | 'toolCallCutoff'
  | 'retryTimeout'
  | 'failureTimeout'
  | 'stopHookBlockCap';

type NumberSettings = Record<NumberSetting, string>;

type NumberField = {
  configKey: string;
  min: number;
  max: number;
  step?: number;
  integer: boolean;
  toConfig: (value: number) => number;
  display: (configValue: unknown) => string;
};

const numericString = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i;

// Quoted YAML numbers such as "20" are read back as strings; goose parses them as numbers.
function parseConfigNumber(value: unknown) {
  const number =
    typeof value === 'string' && numericString.test(value.trim()) ? Number(value) : value;
  return typeof number === 'number' && Number.isFinite(number) ? number : undefined;
}

function readNumber(value: unknown, fallback: number) {
  return parseConfigNumber(value) ?? fallback;
}

const asIs = (value: number) => value;
const displayNumber = (fallback: number) => (value: unknown) => String(readNumber(value, fallback));

const numberFields: Record<NumberSetting, NumberField> = {
  maxTurns: {
    configKey: 'GOOSE_MAX_TURNS',
    min: 1,
    max: 10000,
    integer: true,
    toConfig: asIs,
    display: displayNumber(1000),
  },
  compactionThreshold: {
    configKey: 'GOOSE_AUTO_COMPACT_THRESHOLD',
    min: 1,
    max: 99,
    step: 0.1,
    integer: false,
    toConfig: (percent) => percent / 100,
    display: (value) => String(Number((readNumber(value, 0.8) * 100).toFixed(2))),
  },
  compactionTokenLimit: {
    configKey: 'GOOSE_AUTO_COMPACT_TOKEN_LIMIT',
    min: 1000,
    max: 10000000,
    step: 1000,
    integer: true,
    toConfig: asIs,
    display: displayNumber(225000),
  },
  toolCallCutoff: {
    configKey: 'GOOSE_TOOL_CALL_CUTOFF',
    min: 1,
    max: 100000,
    integer: true,
    toConfig: asIs,
    display: (value) => String(parseConfigNumber(value) ?? ''),
  },
  retryTimeout: {
    configKey: 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS',
    min: 1,
    max: 3600,
    integer: true,
    toConfig: asIs,
    display: displayNumber(300),
  },
  failureTimeout: {
    configKey: 'GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS',
    min: 1,
    max: 3600,
    integer: true,
    toConfig: asIs,
    display: displayNumber(600),
  },
  stopHookBlockCap: {
    configKey: 'GOOSE_STOP_HOOK_BLOCK_CAP',
    min: 1,
    max: 100,
    integer: true,
    toConfig: asIs,
    display: displayNumber(8),
  },
};

const numberFieldEntries = Object.entries(numberFields) as [NumberSetting, NumberField][];

const defaultNumberSettings = Object.fromEntries(
  numberFieldEntries.map(([setting, field]) => [setting, field.display(null)])
) as NumberSettings;

function OperationRow({
  title,
  description,
  enabled,
  onEnabledChange,
  children,
}: {
  title: string;
  description: string;
  enabled?: boolean;
  onEnabledChange?: (checked: boolean) => void;
  children?: ReactNode;
}) {
  const intl = useIntl();

  return (
    <div className="border-t border-border-primary py-3 first:border-t-0">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          {enabled !== undefined && onEnabledChange && (
            <input
              type="checkbox"
              aria-label={intl.formatMessage(i18n.enableOperation, { operation: title })}
              checked={enabled}
              onChange={(event) => onEnabledChange(event.target.checked)}
              className="h-4 w-4 cursor-pointer rounded border-border-primary accent-bgApp"
            />
          )}
          <h4 className="text-sm font-medium text-text-primary">{title}</h4>
        </div>
        <p className="mt-0.5 text-xs text-text-secondary">{description}</p>
      </div>
      {children && <div className="mt-3 flex items-center gap-3">{children}</div>}
    </div>
  );
}

function NumberInput({
  label,
  value,
  min,
  max,
  step = 1,
  placeholder,
  disabled = false,
  onChange,
  onBlur,
}: {
  label: string;
  value: string;
  min: number;
  max: number;
  step?: number;
  placeholder?: string;
  disabled?: boolean;
  onChange: (value: string) => void;
  onBlur: () => void;
}) {
  return (
    <label className="flex items-center gap-2 text-xs text-text-secondary">
      <span>{label}</span>
      <Input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        placeholder={placeholder}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onBlur={onBlur}
        className="w-24"
      />
    </label>
  );
}

export default function AgentLoopSettings() {
  const intl = useIntl();
  const { read, remove, upsert } = useConfig();
  const [slashCommandsEnabled, setSlashCommandsEnabled] = useState(true);
  const [autoEffortEnabled, setAutoEffortEnabled] = useState(false);
  const [typesafeApiKey, setTypesafeApiKey] = useState('');
  const [typesafeApiKeyConfigured, setTypesafeApiKeyConfigured] = useState(false);
  const [toolPairCompactionEnabled, setToolPairCompactionEnabled] = useState(false);
  const [numbers, setNumbers] = useState(defaultNumberSettings);
  // Per field, the value last loaded or sent to config ('' while a removal is pending, null after
  // a failed write). A blur only writes when the input differs from it, so leaving a field
  // unchanged never pins the displayed effective value into the user's config.
  const requestedNumbers = useRef<Record<NumberSetting, string | null>>(defaultNumberSettings);
  // Writes for one field run in order so that the last edit is the one that persists.
  const numberWrites = useRef<Partial<Record<NumberSetting, Promise<void>>>>({});

  useEffect(() => {
    let active = true;

    Promise.all([
      Promise.all(
        numberFieldEntries.map(async ([setting, field]) => [
          setting,
          field.display(await read(field.configKey, false)),
        ])
      ),
      read('GOOSE_SLASH_COMMANDS_ENABLED', false),
      read('GOOSE_TOOL_PAIR_SUMMARIZATION', false),
      read('GOOSE_AUTO_EFFORT_ENABLED', false),
      read('TYPESAFE_API_KEY', true),
    ]).then(([loadedNumbers, slashCommands, toolPairCompaction, autoEffort, typesafeApiKey]) => {
      if (!active) return;

      setSlashCommandsEnabled(typeof slashCommands === 'boolean' ? slashCommands : true);
      setAutoEffortEnabled(typeof autoEffort === 'boolean' ? autoEffort : false);
      setTypesafeApiKeyConfigured(hasConfiguredSecret(typesafeApiKey));
      setToolPairCompactionEnabled(
        typeof toolPairCompaction === 'boolean' ? toolPairCompaction : false
      );
      const loaded = Object.fromEntries(loadedNumbers) as NumberSettings;
      requestedNumbers.current = loaded;
      setNumbers(loaded);
    });

    return () => {
      active = false;
    };
  }, [read]);

  const setNumber = (setting: NumberSetting, value: string) => {
    setNumbers((current) => ({ ...current, [setting]: value }));
  };

  const writeNumber = (setting: NumberSetting, requested: string, write: () => Promise<void>) => {
    requestedNumbers.current = { ...requestedNumbers.current, [setting]: requested };
    const previous = numberWrites.current[setting] ?? Promise.resolve();
    const next = previous
      .catch(() => {})
      .then(write)
      .catch((error) => {
        if (requestedNumbers.current[setting] === requested) {
          requestedNumbers.current = { ...requestedNumbers.current, [setting]: null };
        }
        throw error;
      });
    numberWrites.current[setting] = next;
    return next;
  };

  const resetNumber = async (setting: NumberSetting) => {
    if (requestedNumbers.current[setting] === '') return;
    const { configKey, display } = numberFields[setting];
    await writeNumber(setting, '', async () => {
      await remove(configKey, false);
      const effective = display(await read(configKey, false, { throwOnError: true }));
      if (requestedNumbers.current[setting] !== '') return;
      requestedNumbers.current = { ...requestedNumbers.current, [setting]: effective };
      setNumbers((current) =>
        current[setting].trim() === '' ? { ...current, [setting]: effective } : current
      );
    });
  };

  const saveNumber = async (setting: NumberSetting) => {
    const input = numbers[setting].trim();
    if (input === '') {
      await resetNumber(setting);
      return;
    }
    // Compare before clamping: an out-of-range value from config (e.g. a threshold of 0, which
    // disables compaction) must survive an unchanged blur.
    if (input === requestedNumbers.current[setting]) return;
    const { configKey, min, max, integer, toConfig } = numberFields[setting];
    const parsed = Number(input);
    const clamped = Math.min(max, Math.max(min, Number.isFinite(parsed) ? parsed : min));
    const value = integer ? Math.round(clamped) : clamped;
    setNumber(setting, String(value));
    if (String(value) === requestedNumbers.current[setting]) return;
    await writeNumber(setting, String(value), () => upsert(configKey, toConfig(value), false));
  };

  const numberInputProps = (setting: NumberSetting) => {
    const { min, max, step } = numberFields[setting];
    return {
      value: numbers[setting],
      min,
      max,
      step,
      onChange: (value: string) => setNumber(setting, value),
      onBlur: () => saveNumber(setting),
    };
  };

  const handleToolPairCompactionToggle = async (checked: boolean) => {
    setToolPairCompactionEnabled(checked);
    await upsert('GOOSE_TOOL_PAIR_SUMMARIZATION', checked, false);
  };

  const handleSlashCommandsToggle = async (checked: boolean) => {
    setSlashCommandsEnabled(checked);
    await upsert('GOOSE_SLASH_COMMANDS_ENABLED', checked, false);
  };

  const handleAutoEffortToggle = async (checked: boolean) => {
    setAutoEffortEnabled(checked);
    await upsert('GOOSE_AUTO_EFFORT_ENABLED', checked, false);
  };

  const saveTypesafeApiKey = async () => {
    const apiKey = typesafeApiKey.trim();
    if (!apiKey) return;
    await upsert('TYPESAFE_API_KEY', apiKey, true);
    setTypesafeApiKey('');
    setTypesafeApiKeyConfigured(true);
  };

  const removeTypesafeApiKey = async () => {
    await remove('TYPESAFE_API_KEY', true);
    setTypesafeApiKey('');
    setTypesafeApiKeyConfigured(false);
  };

  return (
    <section className="pr-4">
      <Card className="rounded-xl">
        <CardHeader>
          <CardTitle>{intl.formatMessage(i18n.title)}</CardTitle>
        </CardHeader>

        <CardContent className="px-4">
          <h3 className="mb-1 text-xs font-semibold uppercase tracking-wider text-text-secondary">
            {intl.formatMessage(i18n.operationsHeading)}
          </h3>

          <OperationRow
            title={intl.formatMessage(i18n.slashCommandsTitle)}
            description={intl.formatMessage(i18n.slashCommandsDescription)}
            enabled={slashCommandsEnabled}
            onEnabledChange={handleSlashCommandsToggle}
          />

          <OperationRow
            title={intl.formatMessage(i18n.autoEffortTitle)}
            description={intl.formatMessage(i18n.autoEffortDescription)}
            enabled={autoEffortEnabled}
            onEnabledChange={handleAutoEffortToggle}
          >
            <div className="flex w-full max-w-xl flex-col gap-2">
              <label className="text-xs text-text-secondary">
                {intl.formatMessage(i18n.typesafeApiKeyLabel)}
                {typesafeApiKeyConfigured && (
                  <span className="ml-2 text-green-600">
                    {intl.formatMessage(i18n.apiKeyConfigured)}
                  </span>
                )}
              </label>
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  type="password"
                  value={typesafeApiKey}
                  disabled={!autoEffortEnabled}
                  placeholder={intl.formatMessage(i18n.typesafeApiKeyPlaceholder)}
                  onChange={(event) => setTypesafeApiKey(event.target.value)}
                />
                <Button
                  size="sm"
                  disabled={!autoEffortEnabled || !typesafeApiKey.trim()}
                  onClick={saveTypesafeApiKey}
                >
                  {intl.formatMessage(i18n.saveApiKey)}
                </Button>
                {typesafeApiKeyConfigured && (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={!autoEffortEnabled}
                    onClick={removeTypesafeApiKey}
                  >
                    {intl.formatMessage(i18n.removeApiKey)}
                  </Button>
                )}
              </div>
            </div>
          </OperationRow>

          <OperationRow
            title={intl.formatMessage(i18n.maxTurnsTitle)}
            description={intl.formatMessage(i18n.maxTurnsDescription)}
          >
            <NumberInput
              label={intl.formatMessage(i18n.turnsLabel)}
              {...numberInputProps('maxTurns')}
            />
          </OperationRow>

          <OperationRow
            title={intl.formatMessage(i18n.contextCompactionTitle)}
            description={intl.formatMessage(i18n.contextCompactionDescription)}
          >
            <div className="flex flex-wrap items-center gap-4">
              <NumberInput
                label={intl.formatMessage(i18n.thresholdLabel)}
                {...numberInputProps('compactionThreshold')}
              />
              <NumberInput
                label={intl.formatMessage(i18n.tokenLimitLabel)}
                {...numberInputProps('compactionTokenLimit')}
              />
            </div>
          </OperationRow>

          <OperationRow
            title={intl.formatMessage(i18n.toolPairCompactionTitle)}
            description={intl.formatMessage(i18n.toolPairCompactionDescription)}
            enabled={toolPairCompactionEnabled}
            onEnabledChange={handleToolPairCompactionToggle}
          >
            <div className="flex flex-wrap items-center gap-4">
              <NumberInput
                label={intl.formatMessage(i18n.cutoffLabel)}
                {...numberInputProps('toolCallCutoff')}
                placeholder={intl.formatMessage(i18n.autoPlaceholder)}
                disabled={!toolPairCompactionEnabled}
              />
            </div>
          </OperationRow>

          <OperationRow
            title={intl.formatMessage(i18n.recipeRetryTitle)}
            description={intl.formatMessage(i18n.recipeRetryDescription)}
          >
            <div className="flex flex-col gap-2">
              <NumberInput
                label={intl.formatMessage(i18n.retryTimeoutLabel)}
                {...numberInputProps('retryTimeout')}
              />
              <NumberInput
                label={intl.formatMessage(i18n.failureTimeoutLabel)}
                {...numberInputProps('failureTimeout')}
              />
            </div>
          </OperationRow>

          <OperationRow
            title={intl.formatMessage(i18n.stopHooksTitle)}
            description={intl.formatMessage(i18n.stopHooksDescription)}
          >
            <NumberInput
              label={intl.formatMessage(i18n.blockLimitLabel)}
              {...numberInputProps('stopHookBlockCap')}
            />
          </OperationRow>
        </CardContent>
      </Card>
    </section>
  );
}
