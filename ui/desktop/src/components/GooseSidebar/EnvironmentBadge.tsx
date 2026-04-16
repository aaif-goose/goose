import React from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  dev: {
    id: 'environmentBadge.dev',
    defaultMessage: 'Dev',
  },
});

interface EnvironmentBadgeProps {
  className?: string;
}

const EnvironmentBadge: React.FC<EnvironmentBadgeProps> = ({ className = '' }) => {
  const intl = useIntl();
  const isDevelopment = import.meta.env.DEV;

  if (!isDevelopment) {
    return null;
  }

  const tooltipText = intl.formatMessage(i18n.dev);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className={`relative cursor-default no-drag ${className}`}
          data-testid="environment-badge"
          aria-label={tooltipText}
        >
          <div className="absolute -inset-1" />
          <div className="bg-orange-400 w-2 h-2 rounded-full" />
        </div>
      </TooltipTrigger>
      <TooltipContent
        side="bottom"
        className="bg-orange-400"
        arrowClassName="fill-orange-400 bg-orange-400"
      >
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  );
};

export default EnvironmentBadge;
