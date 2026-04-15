import { AlertCircle, Loader2 } from 'lucide-react';
import React from 'react';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  error: {
    id: 'sessionIndicators.error',
    defaultMessage: 'Session encountered an error',
  },
  streaming: {
    id: 'sessionIndicators.streaming',
    defaultMessage: 'Streaming',
  },
  newActivity: {
    id: 'sessionIndicators.newActivity',
    defaultMessage: 'Has new activity',
  },
});

interface SessionIndicatorsProps {
  isStreaming: boolean;
  hasUnread: boolean;
  hasError: boolean;
}

/**
 * Visual indicators for session status (priority order: error > streaming > unread)
 */
export const SessionIndicators = React.memo<SessionIndicatorsProps>(
  ({ isStreaming, hasUnread, hasError }) => {
    const intl = useIntl();

    if (hasError) {
      return (
        <div className="flex items-center gap-1">
          <AlertCircle
            className="w-3.5 h-3.5 text-red-500"
            aria-label={intl.formatMessage(i18n.error)}
          />
        </div>
      );
    }

    if (isStreaming) {
      return (
        <div className="flex items-center gap-1">
          <Loader2 className="w-3 h-3 text-blue-500 animate-spin" aria-label={intl.formatMessage(i18n.streaming)} />
        </div>
      );
    }

    if (hasUnread) {
      return (
        <div className="flex items-center gap-1">
          <div className="w-2 h-2 bg-green-500 rounded-full" aria-label={intl.formatMessage(i18n.newActivity)} />
        </div>
      );
    }

    return null;
  }
);

SessionIndicators.displayName = 'SessionIndicators';
