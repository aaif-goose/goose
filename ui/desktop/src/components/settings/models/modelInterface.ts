import { listLocalModels } from '../../../acp/local-inference';
import { acpGetProviderDetails, acpListProviderModels } from '../../../acp/providers';
import type { ProviderDetails, ThinkingEffort } from '../../../types/providers';
import { errorMessage as getErrorMessage } from '../../../utils/conversionUtils';

export default interface Model {
  id?: number; // Make `id` optional to allow user-defined models
  name: string;
  provider: string;
  lastUsed?: string;
  alias?: string; // optional model display name
  subtext?: string; // goes below model name if not the provider
  context_limit?: number; // optional context limit override
  reasoning?: boolean; // optional reasoning/thinking support metadata
  request_params?: Record<string, unknown> & { thinking_effort?: ThinkingEffort }; // provider-specific request parameters
}

export async function getProviderMetadata(providerName: string) {
  return (await acpGetProviderDetails(providerName)).metadata;
}

export interface ProviderModelsResult {
  provider: ProviderDetails;
  models: Model[] | null;
  error: string | null;
  warning: string | null;
}

export async function fetchModelsForProviders(
  activeProviders: ProviderDetails[]
): Promise<ProviderModelsResult[]> {
  const modelPromises = activeProviders.map(async (p) => {
    try {
      // For local provider, use listLocalModels and filter to only downloaded models
      if (p.name === 'local') {
        const allModels = await listLocalModels();
        const downloadedModels = allModels
          .filter((m) => m.status.state === 'Downloaded')
          .map((m) => ({ name: m.id, provider: p.name }) as Model);
        return { provider: p, models: downloadedModels, error: null, warning: null };
      }

      const providerModels = await acpListProviderModels(p.name);
      const models = providerModels.map(
        (m) =>
          ({
            name: m.id,
            provider: p.name,
            context_limit: m.contextLimit ?? undefined,
            reasoning: m.reasoning ?? undefined,
          }) as Model
      );
      return { provider: p, models, error: null, warning: null };
    } catch (e: unknown) {
      // For custom providers, fall back to the configured model list
      if (p.provider_type === 'Custom') {
        const fallbackModels = p.metadata.known_models.map(
          (m) =>
            ({
              name: m.name,
              provider: p.name,
              context_limit: m.context_limit,
              reasoning: m.reasoning ?? undefined,
            }) as Model
        );
        if (fallbackModels.length > 0) {
          console.warn(`Failed to fetch models for ${p.name}:`, getErrorMessage(e));
          return {
            provider: p,
            models: fallbackModels,
            error: null,
            warning: `Could not fetch models from provider — showing configured models instead.`,
          };
        }
      }

      const errMsg = getErrorMessage(e);
      const errorMessage = `Failed to fetch models for ${p.name}${errMsg ? `: ${errMsg}` : ''}`;
      return {
        provider: p,
        models: null,
        error: errorMessage,
        warning: null,
      };
    }
  });

  return await Promise.all(modelPromises);
}

export async function fetchModelReasoning(
  provider: string,
  model: string,
  fallback?: boolean
): Promise<boolean | null> {
  try {
    const models = await acpListProviderModels(provider);
    const match = models.find((m) => m.id === model);
    return match?.reasoning ?? fallback ?? null;
  } catch {
    return fallback ?? null;
  }
}
