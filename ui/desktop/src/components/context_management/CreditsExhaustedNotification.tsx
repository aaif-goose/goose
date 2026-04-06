import React from 'react';
import { AlertTriangle, ExternalLink } from 'lucide-react';
import { Message, SystemNotificationContent } from '../../api';
import { WEB_PROTOCOLS } from '../../utils/urlSecurity';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  insufficientCredits: {
    id: 'creditsExhaustedNotification.insufficientCredits',
    defaultMessage: 'Insufficient Credits',
  },
  addCredits: {
    id: 'creditsExhaustedNotification.addCredits',
    defaultMessage: 'Add credits',
  },
});

interface CreditsExhaustedNotificationProps {
  notification: SystemNotificationContent;
}

function getValidatedTopUpUrl(data: unknown): string | null {
  if (!data || typeof data !== 'object') {
    return null;
  }

  const rawUrl = (data as Record<string, unknown>).top_up_url;
  if (typeof rawUrl !== 'string') {
    return null;
  }

  const url = rawUrl.trim();
  if (!url) {
    return null;
  }

  try {
    const parsedUrl = new URL(url);
    if (!WEB_PROTOCOLS.includes(parsedUrl.protocol)) {
      return null;
    }
    return parsedUrl.toString();
  } catch {
    return null;
  }
}

export const CreditsExhaustedNotification: React.FC<CreditsExhaustedNotificationProps> = ({
  notification,
}) => {
  const intl = useIntl();
  const topUpUrl = getValidatedTopUpUrl(notification.data);

  const handleTopUp = () => {
    if (topUpUrl) {
      window.electron.openExternal(topUpUrl);
    }
  };

  return (
    <div className="rounded-lg border border-yellow-600/30 dark:border-yellow-500/30 bg-yellow-500/10 dark:bg-yellow-500/10 p-4 my-2">
      <div className="flex items-start gap-3">
        <AlertTriangle className="h-4 w-4 text-yellow-600 dark:text-yellow-400 mt-0.5 shrink-0" />
        <div className="flex-1">
          <div className="text-sm font-semibold text-yellow-800 dark:text-yellow-200">
            {intl.formatMessage(i18n.insufficientCredits)}
          </div>
          <div className="text-sm text-yellow-800/80 dark:text-yellow-200/80 mt-1">
            {notification.msg}
          </div>
          {topUpUrl && (
            <button
              onClick={handleTopUp}
              className="mt-3 inline-flex items-center gap-2 rounded-md bg-yellow-600 hover:bg-yellow-500 dark:bg-yellow-700 dark:hover:bg-yellow-600 text-white text-sm font-medium px-4 py-2 transition-colors"
            >
              {intl.formatMessage(i18n.addCredits)}
              <ExternalLink className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
};

export function getCreditsExhaustedNotification(
  message: Message
): SystemNotificationContent | undefined {
  return message.content.find(
    (content): content is SystemNotificationContent & { type: 'systemNotification' } =>
      content.type === 'systemNotification' && content.notificationType === 'creditsExhausted'
  );
}
