import { useEffect, useRef, useState } from 'react';
import { fetchCanonicalModelInfo } from '../utils/canonical';
import { Session } from '../api';

interface UseCostTrackingProps {
  sessionInputTokens: number;
  sessionOutputTokens: number;
  localInputTokens: number;
  localOutputTokens: number;
  session?: Session | null;
}

export const useCostTracking = ({
  sessionInputTokens,
  sessionOutputTokens,
  localInputTokens,
  localOutputTokens,
  session,
}: UseCostTrackingProps) => {
  const [sessionCosts, setSessionCosts] = useState<{
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  }>({});

  const currentModel = session?.model_config?.model_name ?? undefined;
  const currentProvider = session?.provider_name ?? undefined;
  const prevModelRef = useRef<string | undefined>(undefined);
  const prevProviderRef = useRef<string | undefined>(undefined);

  // Handle model changes and accumulate costs
  useEffect(() => {
    if (!currentModel || !currentProvider) return;

    const handleModelChange = async () => {
      if (
        prevModelRef.current !== undefined &&
        prevProviderRef.current !== undefined &&
        (prevModelRef.current !== currentModel || prevProviderRef.current !== currentProvider)
      ) {
        // Model/provider has changed, save the costs for the previous model
        const prevKey = `${prevProviderRef.current}/${prevModelRef.current}`;

        // Get pricing info for the previous model
        const prevCostInfo = await fetchCanonicalModelInfo(
          prevProviderRef.current,
          prevModelRef.current
        );

        if (prevCostInfo) {
          const prevInputCost =
            ((sessionInputTokens || localInputTokens) * (prevCostInfo.input_token_cost || 0)) /
            1_000_000;
          const prevOutputCost =
            ((sessionOutputTokens || localOutputTokens) * (prevCostInfo.output_token_cost || 0)) /
            1_000_000;
          const prevTotalCost = prevInputCost + prevOutputCost;

          // Save the accumulated costs for this model
          setSessionCosts((prev) => ({
            ...prev,
            [prevKey]: {
              inputTokens: sessionInputTokens || localInputTokens,
              outputTokens: sessionOutputTokens || localOutputTokens,
              totalCost: prevTotalCost,
            },
          }));
        }

      }

      prevModelRef.current = currentModel || undefined;
      prevProviderRef.current = currentProvider || undefined;
    };

    handleModelChange();
  }, [
    currentModel,
    currentProvider,
    sessionInputTokens,
    sessionOutputTokens,
    localInputTokens,
    localOutputTokens,
    session,
  ]);

  return {
    sessionCosts,
  };
};
