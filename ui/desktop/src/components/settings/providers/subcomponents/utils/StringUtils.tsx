import { defineMessages, useIntl } from '../../../../../i18n';
import type { IntlShape } from 'react-intl';

const i18n = defineMessages({
  ollamaNotConfiguredPrefix: {
    id: 'stringUtils.ollamaNotConfiguredPrefix',
    defaultMessage: 'To use, either the',
  },
  ollamaApp: {
    id: 'stringUtils.ollamaApp',
    defaultMessage: 'Ollama app',
  },
  ollamaNotConfiguredSuffix: {
    id: 'stringUtils.ollamaNotConfiguredSuffix',
    defaultMessage: 'must be installed on your machine and open, or you must enter a value for OLLAMA_HOST.',
  },
  configuredProvider: {
    id: 'stringUtils.configuredProvider',
    defaultMessage: '{name} provider is configured',
  },
});

// Functions for string / string-based element creation (e.g. tooltips for each provider, descriptions, etc)
export function OllamaNotConfiguredTooltipMessage() {
  const intl = useIntl();
  return (
    <p>
      {intl.formatMessage(i18n.ollamaNotConfiguredPrefix)}{' '}
      <a
        href="https://ollama.com/download"
        target="_blank"
        rel="noopener noreferrer"
        className="text-blue-600 underline hover:text-blue-800"
      >
        {intl.formatMessage(i18n.ollamaApp)}
      </a>{' '}
      {intl.formatMessage(i18n.ollamaNotConfiguredSuffix)}
    </p>
  );
}

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
