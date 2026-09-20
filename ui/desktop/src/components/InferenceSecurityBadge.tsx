import { ShieldCheck } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import type { MessageMetadata } from '../types/message';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';

const i18n = defineMessages({
  label: {
    id: 'inferenceSecurityBadge.label',
    defaultMessage: 'Attested TEE',
  },
  description: {
    id: 'inferenceSecurityBadge.description',
    defaultMessage:
      'The model connection for this response was bound to a verified trusted execution environment (TEE). Tool execution is outside this guarantee.',
  },
});

export default function InferenceSecurityBadge({
  security,
}: {
  security: MessageMetadata['inferenceSecurity'];
}) {
  const intl = useIntl();
  if (security !== 'attested_tee') return null;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="inline-flex shrink-0 items-center rounded p-0.5 text-green-700 dark:text-green-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-label={intl.formatMessage(i18n.label)}
        >
          <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top" className="max-w-xs">
        {intl.formatMessage(i18n.description)}
      </TooltipContent>
    </Tooltip>
  );
}
