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
  sessionCostBreakdown: {
    id: 'costTracker.sessionCostBreakdown',
    defaultMessage: 'Session cost breakdown:',
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
  sessionCosts?: {
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  };
  model: string | null;
  provider: string | null;
}

export function CostTracker({
  inputTokens = 0,
  outputTokens = 0,
  sessionCosts,
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
    // If we have session costs, calculate the total across all models
    if (sessionCosts) {
      let totalCost = 0;

      // Add up all historical costs from different models
      Object.values(sessionCosts).forEach((modelCost) => {
        totalCost += modelCost.totalCost;
      });

      // Add current model cost if we have pricing info
      if (
        costInfo &&
        (costInfo.input_token_cost !== undefined || costInfo.output_token_cost !== undefined)
      ) {
        const currentInputCost = (inputTokens * (costInfo.input_token_cost || 0)) / 1_000_000;
        const currentOutputCost = (outputTokens * (costInfo.output_token_cost || 0)) / 1_000_000;
        totalCost += currentInputCost + currentOutputCost;
      }

      return totalCost;
    }

    // Fallback to simple calculation for current model only
    if (
      !costInfo ||
      (costInfo.input_token_cost === undefined && costInfo.output_token_cost === undefined)
    ) {
      return 0;
    }

    const inputCost = (inputTokens * (costInfo.input_token_cost || 0)) / 1_000_000;
    const outputCost = (outputTokens * (costInfo.output_token_cost || 0)) / 1_000_000;
    const total = inputCost + outputCost;

    return total;
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

  // If no cost info found, try to return a default
  if (
    !costInfo ||
    (costInfo.input_token_cost === undefined && costInfo.output_token_cost === undefined)
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
    // Handle error states first
    if (pricingFailed) {
      return intl.formatMessage(i18n.pricingUnavailable, { model: `${currentProvider}/${currentModel}` });
    }

    // Handle session costs
    if (sessionCosts && Object.keys(sessionCosts).length > 0) {
      // Show session breakdown
      let tooltip = intl.formatMessage(i18n.sessionCostBreakdown) + '\n';

      Object.entries(sessionCosts).forEach(([modelKey, cost]) => {
        const costStr = `${costInfo?.currency || '$'}${cost.totalCost.toFixed(6)}`;
        tooltip += `${modelKey}: ${costStr} (${cost.inputTokens.toLocaleString()} in, ${cost.outputTokens.toLocaleString()} out)\n`;
      });

      // Add current model if it has costs
      if (costInfo && (inputTokens > 0 || outputTokens > 0)) {
        const currentCost =
          (inputTokens * (costInfo.input_token_cost || 0) +
            outputTokens * (costInfo.output_token_cost || 0)) /
          1_000_000;
        if (currentCost > 0) {
          tooltip += `${currentProvider}/${currentModel} (current): ${costInfo.currency || '$'}${currentCost.toFixed(6)} (${inputTokens.toLocaleString()} in, ${outputTokens.toLocaleString()} out)\n`;
        }
      }

      tooltip += '\n' + intl.formatMessage(i18n.totalSessionCost, { cost: `${costInfo?.currency || '$'}${totalCost.toFixed(6)}` });
      return tooltip;
    }

    // Default tooltip for single model
    const currency = costInfo?.currency || '$';
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
