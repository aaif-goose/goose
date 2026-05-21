import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import type { CopilotPrefs } from '../../../api/types.gen';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  customInstructionsTitle: {
    id: 'copilotGeneral.customInstructionsTitle',
    defaultMessage: 'Custom instructions',
  },
  customInstructionsDescription: {
    id: 'copilotGeneral.customInstructionsDescription',
    defaultMessage:
      'Free-form rules appended to every review prompt. Use them to nudge the bot toward what your team actually cares about.',
  },
  customInstructionsPlaceholder: {
    id: 'copilotGeneral.customInstructionsPlaceholder',
    defaultMessage:
      'Example: Don\'t flag TODO comments. Be strict on missing tests. Treat any change under crates/goose/src/security/ as critical.',
  },
  customInstructionsHelper: {
    id: 'copilotGeneral.customInstructionsHelper',
    defaultMessage:
      'These instructions are appended verbatim to the system prompt the review orchestrator sends to the model.',
  },
});

interface Props {
  prefs: CopilotPrefs | null;
  onUpdate: (patch: Partial<CopilotPrefs>) => void;
}

export default function CopilotGeneral({ prefs, onUpdate }: Props) {
  const intl = useIntl();
  const value = prefs?.custom_instructions ?? '';

  return (
    <div className="space-y-4 pr-4 pb-8 mt-1">
      <Card className="rounded-lg">
        <CardHeader className="pb-0">
          <CardTitle className="mb-1">
            {intl.formatMessage(i18n.customInstructionsTitle)}
          </CardTitle>
          <CardDescription>
            {intl.formatMessage(i18n.customInstructionsDescription)}
          </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 px-4 space-y-3">
          <textarea
            value={value}
            disabled={!prefs}
            onChange={(e) => onUpdate({ custom_instructions: e.target.value })}
            placeholder={intl.formatMessage(i18n.customInstructionsPlaceholder)}
            rows={6}
            className="flex w-full rounded-md border focus:border-border-secondary hover:border-border-secondary bg-background-primary px-3 py-2 text-xs transition-colors placeholder:text-text-secondary placeholder:font-light focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 resize-y min-h-[120px]"
          />
          <p className="text-xs text-text-secondary max-w-md">
            {intl.formatMessage(i18n.customInstructionsHelper)}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
