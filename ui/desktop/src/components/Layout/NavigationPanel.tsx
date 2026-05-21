import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { motion } from 'framer-motion';
import { useNavigationContext } from './NavigationContext';
import { useConfig } from '../ConfigContext';
import { useNavigationSessions } from '../../hooks/useNavigationSessions';
import { NAV_ITEMS, getNavItemLabel, type NavItem } from '../../hooks/useNavigationItems';
import { AppEvents } from '../../constants/events';
import { ChatHistorySearch } from '../conversation/ChatHistorySearch';
import { SessionsList } from './navigation/SessionsList';
import type { SessionStatus } from './navigation/types';
import { cn } from '../../utils';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  newChat: {
    id: 'navigationPanel.newChat',
    defaultMessage: 'New Chat',
  },
});

export const Navigation: React.FC<{ className?: string }> = ({ className }) => {
  const intl = useIntl();
  const { isNavExpanded } = useNavigationContext();
  const location = useLocation();
  const { extensionsList } = useConfig();

  const appsExtensionEnabled = !!extensionsList?.find((ext) => ext.name === 'apps')?.enabled;

  const visibleItems = useMemo<NavItem[]>(() => {
    return NAV_ITEMS.filter((item) => {
      if (item.path === '/apps') return appsExtensionEnabled;
      return true;
    });
  }, [appsExtensionEnabled]);

  const isActive = useCallback((path: string) => location.pathname === path, [location.pathname]);

  const {
    recentSessions,
    activeSessionId,
    fetchSessions,
    handleNavClick,
    handleNewChat,
    handleSessionClick,
  } = useNavigationSessions();

  const [sessionStatuses, setSessionStatuses] = useState<Map<string, SessionStatus>>(new Map());

  useEffect(() => {
    const handleStatusUpdate = (event: Event) => {
      const { sessionId, streamState } = (event as CustomEvent).detail;
      setSessionStatuses((prev) => {
        const existing = prev.get(sessionId);
        const shouldMarkUnread = existing?.streamState === 'streaming' && streamState === 'idle';
        const next = new Map(prev);
        next.set(sessionId, {
          streamState,
          hasUnreadActivity: existing?.hasUnreadActivity || shouldMarkUnread,
        });
        return next;
      });
    };

    window.addEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
    return () => window.removeEventListener(AppEvents.SESSION_STATUS_UPDATE, handleStatusUpdate);
  }, []);

  const getSessionStatus = useCallback(
    (sessionId: string) => sessionStatuses.get(sessionId),
    [sessionStatuses]
  );

  const clearUnread = useCallback((sessionId: string) => {
    setSessionStatuses((prev) => {
      const status = prev.get(sessionId);
      if (status?.hasUnreadActivity) {
        const next = new Map(prev);
        next.set(sessionId, { ...status, hasUnreadActivity: false });
        return next;
      }
      return prev;
    });
  }, []);

  const navFocusRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isNavExpanded) {
      fetchSessions();
      requestAnimationFrame(() => navFocusRef.current?.focus());
    }
  }, [isNavExpanded, fetchSessions]);

  const [isChatExpanded, setIsChatExpanded] = useState(true);

  if (!isNavExpanded) return null;

  return (
    <motion.div
      ref={navFocusRef}
      tabIndex={-1}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className={cn('bg-app outline-none flex flex-col gap-[2px] h-full pr-[2px]', className)}
    >
      {/* Top spacer to clear the menu toggle / traffic lights */}
      <div className="bg-background-primary rounded-lg flex-shrink-0 h-[48px] w-full" />

      {/* Search */}
      <div className="w-full px-2 pb-1">
        <ChatHistorySearch
          onSessionClick={handleSessionClick}
          getSessionStatus={getSessionStatus}
          clearUnread={clearUnread}
          activeSessionId={activeSessionId}
        />
      </div>

      {/* Nav items */}
      <div className="flex-1 min-h-0 flex flex-col gap-[2px] overflow-y-auto">
        {visibleItems.map((item) => {
          const Icon = item.icon;
          const active = isActive(item.path);
          const isChatItem = item.id === 'chat';

          if (isChatItem) {
            return (
              <div key={item.id} className="w-full flex-shrink-0">
                <div className="relative group">
                  <button
                    onClick={() => setIsChatExpanded((v) => !v)}
                    className={cn(
                      'flex flex-row items-center gap-2 outline-none',
                      'relative rounded-lg transition-colors duration-200 no-drag',
                      'w-full pl-3 pr-3 py-2.5',
                      active
                        ? 'bg-background-inverse text-text-inverse'
                        : 'bg-background-primary hover:bg-background-tertiary'
                    )}
                  >
                    <Icon className="w-5 h-5 flex-shrink-0" />
                    <span className="text-sm font-medium text-left flex-1">
                      {getNavItemLabel(item, intl)}
                    </span>
                    <div className="flex-shrink-0">
                      {isChatExpanded ? (
                        <ChevronDown className="w-3 h-3 text-text-secondary" />
                      ) : (
                        <ChevronRight className="w-3 h-3 text-text-secondary" />
                      )}
                    </div>
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleNewChat();
                    }}
                    className={cn(
                      'absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-md z-10',
                      'opacity-0 group-hover:opacity-100 transition-opacity',
                      active
                        ? 'hover:bg-white/20 text-text-inverse'
                        : 'hover:bg-background-tertiary text-text-primary'
                    )}
                    title={intl.formatMessage(i18n.newChat)}
                  >
                    <Plus className="w-4 h-4" />
                  </button>
                </div>
                <SessionsList
                  sessions={recentSessions}
                  activeSessionId={activeSessionId}
                  isExpanded={isChatExpanded}
                  getSessionStatus={getSessionStatus}
                  clearUnread={clearUnread}
                  onSessionClick={handleSessionClick}
                  onSessionRenamed={fetchSessions}
                  onShowAll={() => handleNavClick('/sessions')}
                />
              </div>
            );
          }

          return (
            <button
              key={item.id}
              onClick={() => handleNavClick(item.path)}
              className={cn(
                'flex flex-row items-center gap-2 outline-none',
                'relative rounded-lg transition-colors duration-200 no-drag w-full',
                'pl-3 pr-3 py-2.5',
                active
                  ? 'bg-background-inverse text-text-inverse'
                  : 'bg-background-primary hover:bg-background-tertiary'
              )}
            >
              <Icon className="w-5 h-5 flex-shrink-0" />
              <span className="text-sm font-medium text-left flex-1">
                {getNavItemLabel(item, intl)}
              </span>
              {item.getTag && (
                <span className="text-xs font-mono text-text-secondary">{item.getTag()}</span>
              )}
            </button>
          );
        })}
      </div>
    </motion.div>
  );
};
