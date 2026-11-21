import { useState, type ReactNode } from 'react';
import { defineMessages, useIntl } from '../i18n';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import Expand from './ui/Expand';
import type { TurnActivity } from './turnActivity';
import type { ShowThinking } from '../utils/settings';

const i18n = defineMessages({
  count: {
    id: 'turnActivity.count',
    defaultMessage: '{count, plural, one {# tool call} other {# tool calls}}',
  },
  activity: { id: 'turnActivity.activity', defaultMessage: 'Activity' },
  running: { id: 'turnActivity.running', defaultMessage: 'Running' },
  completed: { id: 'turnActivity.completed', defaultMessage: 'Completed' },
  failed: { id: 'turnActivity.failed', defaultMessage: 'Failed' },
  incomplete: { id: 'turnActivity.incomplete', defaultMessage: 'Incomplete' },
  waiting: { id: 'turnActivity.waiting', defaultMessage: 'Waiting for approval' },
});

export default function TurnActivityRow({
  turn,
  showThinking,
  isSearching,
  renderSteps,
}: {
  turn: TurnActivity;
  showThinking: ShowThinking | null;
  isSearching: boolean;
  renderSteps: () => ReactNode;
}) {
  const intl = useIntl();
  const thinkingSetting = showThinking ?? 'collapsed';
  const [previousSetting, setPreviousSetting] = useState(thinkingSetting);
  const [manualExpansion, setManualExpansion] = useState<boolean | null>(null);
  if (previousSetting !== thinkingSetting) {
    setPreviousSetting(thinkingSetting);
    setManualExpansion(null);
  }
  const openForThinking =
    thinkingSetting === 'always' &&
    turn.steps.some(({ message }) =>
      message.content.some((content) => content.type === 'thinking')
    );
  const expanded = isSearching || (manualExpansion ?? openForThinking);
  const label = (
    <>
      <span>
        {turn.toolCount > 0
          ? intl.formatMessage(i18n.count, { count: turn.toolCount })
          : intl.formatMessage(i18n.activity)}
      </span>
      <span aria-hidden="true">·</span>
      <span>{intl.formatMessage(i18n[turn.status])}</span>
    </>
  );

  return (
    <Collapsible open={expanded} onOpenChange={setManualExpansion} className="mt-3">
      {turn.steps.length > 0 ? (
        <>
          <CollapsibleTrigger
            disabled={isSearching}
            className="flex min-h-7 max-w-full cursor-pointer flex-wrap items-center gap-x-1.5 rounded-sm text-start text-xs text-text-secondary hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-border-active disabled:cursor-default"
          >
            <span className="rtl:scale-x-[-1]" aria-hidden="true">
              <Expand size={3} isExpanded={expanded} />
            </span>
            {label}
          </CollapsibleTrigger>
          <CollapsibleContent className="my-1 space-y-2 border-s border-border-primary ps-3">
            {expanded && renderSteps()}
          </CollapsibleContent>
        </>
      ) : (
        <div className="flex min-h-7 flex-wrap items-center gap-x-1.5 text-xs text-text-secondary">
          {label}
        </div>
      )}
    </Collapsible>
  );
}
