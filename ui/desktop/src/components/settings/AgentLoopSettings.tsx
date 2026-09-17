import { type ReactNode, useEffect, useState } from 'react';
import { useConfig } from '../ConfigContext';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../ui/card';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  title: {
    id: 'settings.agentLoop.title',
    defaultMessage: 'Agent Loop',
  },
  description: {
    id: 'settings.agentLoop.description',
    defaultMessage: 'Use the operation-based agent loop. Turn this off to use the legacy loop.',
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
  | 'toolCallCutoff'
  | 'retryTimeout'
  | 'failureTimeout'
  | 'stopHookBlockCap';

type NumberSettings = Record<NumberSetting, string>;

const defaultNumberSettings: NumberSettings = {
  maxTurns: '1000',
  compactionThreshold: '80',
  toolCallCutoff: '',
  retryTimeout: '300',
  failureTimeout: '600',
  stopHookBlockCap: '8',
};

function readNumber(value: unknown, fallback: number) {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

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
  const [enabled, setEnabled] = useState(true);
  const [slashCommandsEnabled, setSlashCommandsEnabled] = useState(true);
  const [toolPairCompactionEnabled, setToolPairCompactionEnabled] = useState(false);
  const [numbers, setNumbers] = useState(defaultNumberSettings);

  useEffect(() => {
    let active = true;

    Promise.all([
      window.electron.getSetting('useLegacyAgentLoop'),
      read('GOOSE_MAX_TURNS', false),
      read('GOOSE_AUTO_COMPACT_THRESHOLD', false),
      read('GOOSE_SLASH_COMMANDS_ENABLED', false),
      read('GOOSE_TOOL_PAIR_SUMMARIZATION', false),
      read('GOOSE_TOOL_CALL_CUTOFF', false),
      read('GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', false),
      read('GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS', false),
      read('GOOSE_STOP_HOOK_BLOCK_CAP', false),
    ]).then(
      ([
        useLegacyAgentLoop,
        maxTurns,
        compactionThreshold,
        slashCommands,
        toolPairCompaction,
        toolCallCutoff,
        retryTimeout,
        failureTimeout,
        stopHookBlockCap,
      ]) => {
        if (!active) return;

        setEnabled(!useLegacyAgentLoop);
        setSlashCommandsEnabled(typeof slashCommands === 'boolean' ? slashCommands : true);
        setToolPairCompactionEnabled(
          typeof toolPairCompaction === 'boolean' ? toolPairCompaction : false
        );
        setNumbers({
          maxTurns: String(readNumber(maxTurns, 1000)),
          compactionThreshold: String(
            Number((readNumber(compactionThreshold, 0.8) * 100).toFixed(2))
          ),
          toolCallCutoff:
            typeof toolCallCutoff === 'number' && Number.isFinite(toolCallCutoff)
              ? String(toolCallCutoff)
              : '',
          retryTimeout: String(readNumber(retryTimeout, 300)),
          failureTimeout: String(readNumber(failureTimeout, 600)),
          stopHookBlockCap: String(readNumber(stopHookBlockCap, 8)),
        });
      }
    );

    return () => {
      active = false;
    };
  }, [read]);

  const setNumber = (setting: NumberSetting, value: string) => {
    setNumbers((current) => ({ ...current, [setting]: value }));
  };

  const saveNumber = async (
    setting: NumberSetting,
    configKey: string,
    min: number,
    max: number,
    serialize: (value: number) => number = (value) => value,
    integer = true
  ) => {
    const parsed = Number(numbers[setting]);
    const clamped = Math.min(max, Math.max(min, Number.isFinite(parsed) ? parsed : min));
    const value = integer ? Math.round(clamped) : clamped;
    setNumber(setting, String(value));
    await upsert(configKey, serialize(value), false);
  };

  const handleAgentLoopToggle = async (checked: boolean) => {
    setEnabled(checked);
    await window.electron.setSetting('useLegacyAgentLoop', !checked);
  };

  const handleToolPairCompactionToggle = async (checked: boolean) => {
    setToolPairCompactionEnabled(checked);
    await upsert('GOOSE_TOOL_PAIR_SUMMARIZATION', checked, false);
  };

  const handleSlashCommandsToggle = async (checked: boolean) => {
    setSlashCommandsEnabled(checked);
    await upsert('GOOSE_SLASH_COMMANDS_ENABLED', checked, false);
  };

  const saveToolCallCutoff = async () => {
    if (numbers.toolCallCutoff.trim() === '') {
      await remove('GOOSE_TOOL_CALL_CUTOFF', false);
      return;
    }
    await saveNumber('toolCallCutoff', 'GOOSE_TOOL_CALL_CUTOFF', 1, 100000);
  };

  return (
    <section className="pr-4">
      <Card className="rounded-xl">
        <CardHeader>
          <div className="flex items-center justify-between gap-6">
            <div>
              <CardTitle>{intl.formatMessage(i18n.title)}</CardTitle>
              <CardDescription className="mt-1">
                {intl.formatMessage(i18n.description)}
              </CardDescription>
            </div>
            <Switch checked={enabled} onCheckedChange={handleAgentLoopToggle} variant="mono" />
          </div>
        </CardHeader>

        {enabled && (
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
              title={intl.formatMessage(i18n.maxTurnsTitle)}
              description={intl.formatMessage(i18n.maxTurnsDescription)}
            >
              <NumberInput
                label={intl.formatMessage(i18n.turnsLabel)}
                value={numbers.maxTurns}
                min={1}
                max={10000}
                onChange={(value) => setNumber('maxTurns', value)}
                onBlur={() => saveNumber('maxTurns', 'GOOSE_MAX_TURNS', 1, 10000)}
              />
            </OperationRow>

            <OperationRow
              title={intl.formatMessage(i18n.contextCompactionTitle)}
              description={intl.formatMessage(i18n.contextCompactionDescription)}
            >
              <NumberInput
                label={intl.formatMessage(i18n.thresholdLabel)}
                value={numbers.compactionThreshold}
                min={1}
                max={99}
                step={0.1}
                onChange={(value) => setNumber('compactionThreshold', value)}
                onBlur={() =>
                  saveNumber(
                    'compactionThreshold',
                    'GOOSE_AUTO_COMPACT_THRESHOLD',
                    1,
                    99,
                    (value) => value / 100,
                    false
                  )
                }
              />
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
                  value={numbers.toolCallCutoff}
                  min={1}
                  max={100000}
                  placeholder={intl.formatMessage(i18n.autoPlaceholder)}
                  disabled={!toolPairCompactionEnabled}
                  onChange={(value) => setNumber('toolCallCutoff', value)}
                  onBlur={saveToolCallCutoff}
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
                  value={numbers.retryTimeout}
                  min={1}
                  max={3600}
                  onChange={(value) => setNumber('retryTimeout', value)}
                  onBlur={() =>
                    saveNumber('retryTimeout', 'GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS', 1, 3600)
                  }
                />
                <NumberInput
                  label={intl.formatMessage(i18n.failureTimeoutLabel)}
                  value={numbers.failureTimeout}
                  min={1}
                  max={3600}
                  onChange={(value) => setNumber('failureTimeout', value)}
                  onBlur={() =>
                    saveNumber('failureTimeout', 'GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS', 1, 3600)
                  }
                />
              </div>
            </OperationRow>

            <OperationRow
              title={intl.formatMessage(i18n.stopHooksTitle)}
              description={intl.formatMessage(i18n.stopHooksDescription)}
            >
              <NumberInput
                label={intl.formatMessage(i18n.blockLimitLabel)}
                value={numbers.stopHookBlockCap}
                min={1}
                max={100}
                onChange={(value) => setNumber('stopHookBlockCap', value)}
                onBlur={() => saveNumber('stopHookBlockCap', 'GOOSE_STOP_HOOK_BLOCK_CAP', 1, 100)}
              />
            </OperationRow>
          </CardContent>
        )}
      </Card>
    </section>
  );
}
