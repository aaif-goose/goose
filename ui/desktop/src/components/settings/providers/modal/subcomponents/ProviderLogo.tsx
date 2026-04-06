import OpenAILogo from './icons/openai@3x.png';
import AnthropicLogo from './icons/anthropic@3x.png';
import GoogleLogo from './icons/google@3x.png';
import GroqLogo from './icons/groq@3x.png';
import OllamaLogo from './icons/ollama@3x.png';
import DatabricksLogo from './icons/databricks@3x.png';
import OpenRouterLogo from './icons/openrouter@3x.png';
import SnowflakeLogo from './icons/snowflake@3x.png';
import XaiLogo from './icons/xai@3x.png';
import MiniMaxLogo from './icons/minimax@3x.png';
import TanzuLogo from './icons/tanzu@3x.png';
import DefaultLogo from './icons/default@3x.png';
import { defineMessages, useIntl } from '../../../../../i18n';

const i18n = defineMessages({
  logoAlt: {
    id: 'providerLogo.alt',
    defaultMessage: '{providerName} logo',
  },
});

// Map provider names to their logos
const providerLogos: Record<string, string> = {
  openai: OpenAILogo,
  anthropic: AnthropicLogo,
  google: GoogleLogo,
  groq: GroqLogo,
  ollama: OllamaLogo,
  databricks: DatabricksLogo,
  openrouter: OpenRouterLogo,
  snowflake: SnowflakeLogo,
  xai: XaiLogo,
  minimax: MiniMaxLogo,
  tanzu_ai: TanzuLogo,
  default: DefaultLogo,
};

interface ProviderLogoProps {
  providerName: string;
}

export default function ProviderLogo({ providerName }: ProviderLogoProps) {
  const intl = useIntl();
  // Convert provider name to lowercase and fetch the logo
  const logoKey = providerName.toLowerCase();
  const logo = providerLogos[logoKey] || DefaultLogo;

  // Special handling for xAI logo
  const isXai = logoKey === 'xai';
  const imageStyle = isXai ? { filter: 'invert(1)', opacity: 0.9 } : {};

  // Use smaller size for xAI logo to fit better in circle
  const imageClassName = isXai
    ? 'w-8 h-8 object-contain' // Smaller size for xAI
    : 'w-16 h-16 object-contain'; // Default size for others

  return (
    <div className="flex justify-center mb-2">
      <div className="w-12 h-12 bg-black rounded-full overflow-hidden flex items-center justify-center">
        <img
          src={logo}
          alt={intl.formatMessage(i18n.logoAlt, { providerName })}
          className={imageClassName}
          style={imageStyle}
        />
      </div>
    </div>
  );
}
