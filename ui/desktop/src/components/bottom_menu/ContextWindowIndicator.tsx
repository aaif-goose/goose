import BottomMenuAlertPopover from './BottomMenuAlertPopover';
import { Alert } from '../alerts';

interface ContextWindowIndicatorProps {
  totalTokens: number;
  tokenLimit: number;
  alerts: Alert[];
}

const formatTokenCount = (count: number): string => {
  if (count >= 1000000) {
    const millions = count / 1000000;
    return millions % 1 === 0 ? `${millions.toFixed(0)}M` : `${millions.toFixed(1)}M`;
  } else if (count >= 1000) {
    const thousands = count / 1000;
    return thousands % 1 === 0 ? `${thousands.toFixed(0)}k` : `${thousands.toFixed(1)}k`;
  }
  return count.toString();
};

const getProgressColor = (percentage: number): string => {
  if (percentage <= 75) return 'text-text-primary/70';
  if (percentage <= 90) return 'text-orange-500';
  return 'text-red-500';
};

export function ContextWindowIndicator({
  totalTokens,
  tokenLimit,
  alerts,
}: ContextWindowIndicatorProps) {
  if (!tokenLimit) return null;

  const percentage = Math.round((totalTokens / tokenLimit) * 100);
  const colorClass = getProgressColor(percentage);

  return (
    <>
      <div className="flex items-center h-full">
        <BottomMenuAlertPopover alerts={alerts}>
          <span className={`text-xs font-mono ${colorClass}`}>
            {formatTokenCount(totalTokens)} / {formatTokenCount(tokenLimit)}
          </span>
        </BottomMenuAlertPopover>
      </div>
      <div className="w-px h-4 bg-border-primary mx-2" />
    </>
  );
}
