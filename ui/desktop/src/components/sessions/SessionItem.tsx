import React from 'react';
import { defineMessages, useIntl } from '../../i18n';
import { Card } from '../ui/card';
import { formatDate } from '../../utils/date';
import { Session } from '../../api';
import { shouldShowNewChatTitle } from '../../sessions';
import { DEFAULT_CHAT_TITLE } from '../../contexts/ChatContext';

const i18n = defineMessages({
  messageCount: {
    id: 'sessionItem.messageCount',
    defaultMessage: '{count} messages',
  },
});

interface SessionItemProps {
  session: Session;
  extraActions?: React.ReactNode;
}

const SessionItem: React.FC<SessionItemProps> = ({ session, extraActions }) => {
  const intl = useIntl();
  const displayName = shouldShowNewChatTitle(session) ? DEFAULT_CHAT_TITLE : session.name;

  return (
    <Card className="p-4 mb-2 hover:bg-background-inverse/50 cursor-pointer flex justify-between items-center">
      <div>
        <div className="font-medium">{displayName}</div>
        <div className="text-sm text-muted-foreground">
          {formatDate(session.updated_at)} • {intl.formatMessage(i18n.messageCount, { count: session.message_count })}
        </div>
        <div className="text-sm text-muted-foreground">{session.working_dir}</div>
      </div>
      {extraActions && <div>{extraActions}</div>}
    </Card>
  );
};

export default SessionItem;
