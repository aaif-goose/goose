import { defineMessages } from '../../../../../i18n';
import type { IntlShape } from 'react-intl';

const i18n = defineMessages({
  configuredProvider: {
    id: 'stringUtils.configuredProvider',
    defaultMessage: '{name} provider is configured',
  },
});

export function ConfiguredProviderTooltipMessage(intl: IntlShape, name: string) {
  return intl.formatMessage(i18n.configuredProvider, { name });
}

interface ProviderDescriptionProps {
  description: string;
}

export function ProviderDescription({ description }: ProviderDescriptionProps) {
  return (
    <p className="text-xs text-text-secondary mt-1.5 mb-3 leading-normal overflow-y-auto max-h-[54px]">
      {description}
    </p>
  );
}
