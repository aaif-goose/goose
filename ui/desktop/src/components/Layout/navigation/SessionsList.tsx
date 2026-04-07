import React, { useState, useCallback } from 'react';
import { MessageSquare, ChefHat, Plus, History } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { SessionIndicators } from '../../SessionIndicators';
import { InlineEditText } from '../../common/InlineEditText';
import { cn } from '../../../utils';
import { getSessionDisplayName } from '../../../hooks/useNavigationSessions';
import { updateSessionName } from '../../../api';
import type { Session } from '../../../api';
import type { SessionStatus } from './types';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  startNewChat: {
    id: 'sessionsList.startNewChat',
    defaultMessage: 'Start New Chat',
  },
  untitledSession: {
    id: 'sessionsList.untitledSession',
    defaultMessage: 'Untitled session',
  },
  showAll: {
    id: 'sessionsList.showAll',
    defaultMessage: 'Show All',
  },
});

interface SessionsListProps {
  sessions: Session[];
  activeSessionId?: string;
  isExpanded: boolean;
  getSessionStatus: (sessionId: string) => SessionStatus | undefined;
  clearUnread: (sessionId: string) => void;
  onSessionClick: (sessionId: string) => void;
  onSessionRenamed?: () => void;
  onNewChat?: () => void;
  onShowAll?: () => void;
}

export const SessionsList: React.FC<SessionsListProps> = ({
  sessions,
  activeSessionId,
  isExpanded,
  getSessionStatus,
  clearUnread,
  onSessionClick,
  onSessionRenamed,
  onNewChat,
  onShowAll,
}) => {
  const intl = useIntl();
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);

  const handleSaveSessionName = useCallback(
    async (sessionId: string, newName: string) => {
      await updateSessionName({
        path: { session_id: sessionId },
        body: { name: newName },
      });
      onSessionRenamed?.();
    },
    [onSessionRenamed]
  );

  return (
    <AnimatePresence>
      {isExpanded && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: 'auto', opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="overflow-hidden mt-[2px]"
        >
          <div className="bg-background-primary rounded-lg py-1 flex flex-col gap-[2px]">
            {/* New Chat button as first item */}
            {onNewChat && (
              <div
                onClick={onNewChat}
                className={cn(
                  'w-full text-left py-1.5 px-2 text-xs rounded-md',
                  'hover:bg-background-tertiary transition-colors',
                  'flex items-center gap-2 cursor-pointer'
                )}
              >
                <div className="w-4 flex-shrink-0" />
                <Plus className="w-4 h-4 flex-shrink-0 text-text-secondary" />
                <span className="text-text-primary">{intl.formatMessage(i18n.startNewChat)}</span>
              </div>
            )}

            {sessions.map((session) => {
              const status = getSessionStatus(session.id);
              const isStreaming = status?.streamState === 'streaming';
              const hasError = status?.streamState === 'error';
              const hasUnread = status?.hasUnreadActivity ?? false;
              const isActiveSession = session.id === activeSessionId;
              const isEditing = editingSessionId === session.id;

              return (
                <div
                  key={session.id}
                  onClick={() => {
                    if (!isEditing) {
                      clearUnread(session.id);
                      onSessionClick(session.id);
                    }
                  }}
                  className={cn(
                    'w-full text-left py-1.5 px-2 text-xs rounded-md',
                    'hover:bg-background-tertiary transition-colors',
                    'flex items-center gap-2 cursor-pointer',
                    isActiveSession && 'bg-background-tertiary'
                  )}
                >
                  <div className="w-4 flex-shrink-0" />
                  {session.recipe ? (
                    <ChefHat className="w-4 h-4 flex-shrink-0 text-text-secondary" />
                  ) : (
                    <MessageSquare className="w-4 h-4 flex-shrink-0 text-text-secondary" />
                  )}
                  <InlineEditText
                    value={getSessionDisplayName(session)}
                    onSave={(newName) => handleSaveSessionName(session.id, newName)}
                    placeholder={intl.formatMessage(i18n.untitledSession)}
                    disabled={isStreaming}
                    singleClickEdit={false}
                    className="truncate text-text-primary flex-1 !px-0 !py-0 hover:bg-transparent"
                    editClassName="!text-xs"
                    onEditStart={() => setEditingSessionId(session.id)}
                    onEditEnd={() => setEditingSessionId(null)}
                  />
                  <SessionIndicators
                    isStreaming={isStreaming}
                    hasUnread={hasUnread}
                    hasError={hasError}
                  />
                </div>
              );
            })}

            {/* Show All button at bottom */}
            {onShowAll && sessions.length > 0 && (
              <div
                onClick={onShowAll}
                className={cn(
                  'w-full text-left py-1.5 px-2 text-xs rounded-md',
                  'hover:bg-background-tertiary transition-colors',
                  'flex items-center gap-2 cursor-pointer text-text-secondary'
                )}
              >
                <div className="w-4 flex-shrink-0" />
                <History className="w-4 h-4 flex-shrink-0" />
                <span>{intl.formatMessage(i18n.showAll)}</span>
              </div>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
