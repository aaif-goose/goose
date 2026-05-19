import { useState, useEffect } from 'react';
import { CoinIcon } from '../icons';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { fetchCanonicalModelInfo } from '../../utils/canonical';
import type { ModelInfoData } from '../../api';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  pricingUnavailable: {
    id: 'costTracker.pricingUnavailable',
    defaultMessage: 'Pricing data unavailable for {model}',
  },
  costUnavailable: {
    id: 'costTracker.costUnavailable',
    defaultMessage: 'Cost data not available for {model} ({inputTokens} input, {outputTokens} output tokens)',
  },
  totalSessionCost: {
    id: 'costTracker.totalSessionCost',
    defaultMessage: 'Total session cost: {cost}',
  },
  inputOutputTooltip: {
    id: 'costTracker.inputOutputTooltip',
    defaultMessage: 'Input: {inputTokens} tokens ({inputCost}) | Output: {outputTokens} tokens ({outputCost})',
  },
});

interface CostTrackerProps {
  inputTokens?: number;
  outputTokens?: number;
  accumulatedCost?: number | null;
  model: string | null;
  provider: string | null;
}

export function CostTracker({
  inputTokens = 0,
  outputTokens = 0,
  accumulatedCost,
  model: currentModel,
  provider: currentProvider,
}: CostTrackerProps) {
  const intl = useIntl();
  const [costInfo, setCostInfo] = useState<ModelInfoData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [showPricing, setShowPricing] = useState(true);
  const [pricingFailed, setPricingFailed] = useState(false);

  // Check if pricing is enabled
  useEffect(() => {
    const loadPricingSetting = async () => {
      const enabled = await window.electron.getSetting('showPricing');
      setShowPricing(enabled);
    };

    loadPricingSetting();

    const handlePricingChange = () => {
      loadPricingSetting();
    };

    window.addEventListener('showPricingChanged', handlePricingChange);
    return () => window.removeEventListener('showPricingChanged', handlePricingChange);
  }, []);

  useEffect(() => {
    const loadCostInfo = async () => {
      if (!currentModel || !currentProvider) {
        setIsLoading(false);
        return;
      }

      setIsLoading(true);
      try {
        const costData = await fetchCanonicalModelInfo(currentProvider, currentModel);
        if (costData) {
          setCostInfo(costData);
          setPricingFailed(false);
        } else {
          setPricingFailed(true);
          setCostInfo(null);
        }
      } catch {
        setPricingFailed(true);
        setCostInfo(null);
      } finally {
        setIsLoading(false);
      }
    };

    loadCostInfo();
  }, [currentModel, currentProvider]);

  // Return null early if pricing is disabled
  if (!showPricing) {
    return null;
  }

  const calculateCost = (): number => {
    return accumulatedCost ?? 0;
  };

  const formatCost = (cost: number): string => {
    // Always show 4 decimal places for consistency
    return cost.toFixed(4);
  };

  // Show loading state or when we don't have model/provider info
  if (!currentModel || !currentProvider) {
    return null;
  }

  // If still loading, show a placeholder
  if (isLoading) {
    return (
      <>
        <div className="flex items-center justify-center h-full text-text-secondary translate-y-[1px]">
          <span className="text-xs font-mono">...</span>
        </div>
        <div className="w-px h-4 bg-border-primary mx-2" />
      </>
    );
  }

  if (
    accumulatedCost == null &&
    (!costInfo ||
      (costInfo.input_token_cost === undefined && costInfo.output_token_cost === undefined))
  ) {
    const freeProviders = ['ollama', 'local', 'localhost'];
    if (freeProviders.includes(currentProvider.toLowerCase())) {
      return (
        <>
          <div className="flex items-center justify-center h-full text-text-primary/70 transition-colors cursor-default translate-y-[1px]">
            <span className="text-xs font-mono">
              {inputTokens.toLocaleString()}↑ {outputTokens.toLocaleString()}↓
            </span>
          </div>
          <div className="w-px h-4 bg-border-primary mx-2" />
        </>
      );
    }

    // Otherwise show as unavailable
    const getUnavailableTooltip = () => {
      if (pricingFailed) {
        return intl.formatMessage(i18n.pricingUnavailable, { model: currentModel });
      }
      return intl.formatMessage(i18n.costUnavailable, {
        model: currentModel,
        inputTokens: inputTokens.toLocaleString(),
        outputTokens: outputTokens.toLocaleString(),
      });
    };

    return (
      <>
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex items-center justify-center h-full transition-colors cursor-default translate-y-[1px] text-text-primary/70 hover:text-text-primary">
              <CoinIcon className="mr-1" size={16} />
              <span className="text-xs font-mono">0.0000</span>
            </div>
          </TooltipTrigger>
          <TooltipContent>{getUnavailableTooltip()}</TooltipContent>
        </Tooltip>
        <div className="w-px h-4 bg-border-primary mx-2" />
      </>
    );
  }

  const totalCost = calculateCost();

  // Build tooltip content
  const getTooltipContent = (): string => {
    if (pricingFailed) {
      return intl.formatMessage(i18n.pricingUnavailable, { model: `${currentProvider}/${currentModel}` });
    }

    const currency = costInfo?.currency || '$';

    if (accumulatedCost != null) {
      return intl.formatMessage(i18n.totalSessionCost, { cost: `${currency}${totalCost.toFixed(4)}` })
        + `\n` + intl.formatMessage(i18n.inputOutputTooltip, {
          inputTokens: inputTokens.toLocaleString(),
          inputCost: `${currency}${((inputTokens * (costInfo?.input_token_cost || 0)) / 1_000_000).toFixed(6)}`,
          outputTokens: outputTokens.toLocaleString(),
          outputCost: `${currency}${((outputTokens * (costInfo?.output_token_cost || 0)) / 1_000_000).toFixed(6)}`,
        });
    }

    const inputCostStr = `${currency}${((inputTokens * (costInfo?.input_token_cost || 0)) / 1_000_000).toFixed(6)}`;
    const outputCostStr = `${currency}${((outputTokens * (costInfo?.output_token_cost || 0)) / 1_000_000).toFixed(6)}`;
    return intl.formatMessage(i18n.inputOutputTooltip, {
      inputTokens: inputTokens.toLocaleString(),
      inputCost: inputCostStr,
      outputTokens: outputTokens.toLocaleString(),
      outputCost: outputCostStr,
    });
  };

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="flex items-center justify-center h-full transition-colors cursor-default translate-y-[1px] text-text-primary/70 hover:text-text-primary">
            <CoinIcon className="mr-1" size={16} />
            <span className="text-xs font-mono">{formatCost(totalCost)}</span>
          </div>
        </TooltipTrigger>
        <TooltipContent>{getTooltipContent()}</TooltipContent>
      </Tooltip>
      <div className="w-px h-4 bg-border-primary mx-2" />
    </>
  );
}
