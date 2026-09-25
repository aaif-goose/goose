import { ShieldCheck } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';
import type { MessageMetadata } from '../types/message';

const i18n = defineMessages({
  label: {
    id: 'inferenceSecurityBadge.label',
    defaultMessage: 'Verified TEE',
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
    <span className="inline-flex shrink-0 items-center gap-1 text-xs font-mono text-text-secondary">
      <ShieldCheck className="h-3.5 w-3.5 text-green-700 dark:text-green-400" aria-hidden="true" />
      {intl.formatMessage(i18n.label)}
    </span>
  );
}
