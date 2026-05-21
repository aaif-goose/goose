import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { ChevronDown, ChevronRight, PanelLeft, Plus, Search } from 'lucide-react';
import { motion } from 'framer-motion';
import { useNavigationContext } from './NavigationContext';
import { useConfig } from '../ConfigContext';
import { useNavigationSessions, getSessionDisplayName } from '../../hooks/useNavigationSessions';
import {
  NAV_ITEMS,
  SETTINGS_NAV_ITEM,
  getNavItemLabel,
  type NavItem,
} from '../../hooks/useNavigationItems';
import { AppEvents } from '../../constants/events';
import { Goose } from '../icons/Goose';
import { InlineEditText } from '../common/InlineEditText';
import { SessionIndicators } from '../SessionIndicators';
import { updateSessionName, type Session } from '../../api';
import { cn } from '../../utils';

type StreamState = 'idle' | 'loading' | 'streaming' | 'error';

interface SessionStatus {
  streamState: StreamState;
  hasUnreadActivity: boolean;
}
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  searchPlaceholder: {
    id: 'navigationPanel.searchPlaceholder',
    defaultMessage: 'Search chats…',
  },
  chats: {
    id: 'navigationPanel.chats',
    defaultMessage: 'Chats',
  },
  noChats: {
    id: 'navigationPanel.noChats',
    defaultMessage: 'No recent chats',
  },
  newChat: {
    id: 'navigationPanel.newChat',
    defaultMessage: 'New chat',
  },
  untitledSession: {
    id: 'navigationPanel.untitledSession',
    defaultMessage: 'Untitled session',
  },
  collapseSidebar: {
    id: 'navigationPanel.collapseSidebar',
    defaultMessage: 'Collapse sidebar',
  },
});

const navItemClass = (active: boolean) =>
  cn(
    'flex flex-row items-center gap-3 outline-none no-drag w-full',
    'rounded-full px-3 py-2 text-sm font-medium transition-colors',
    active ? 'bg-background-tertiary text-text-primary' : 'text-text-primary hover:bg-background-tertiary/60'
  );

interface NavRowProps {
  item: NavItem;
  active: boolean;
  onClick: () => void;
}

const NavRow: React.FC<NavRowProps> = ({ item, active, onClick }) => {
  const intl = useIntl();
  const Icon = item.icon;
  return (
    <button onClick={onClick} className={navItemClass(active)}>
      <Icon className="w-5 h-5 flex-shrink-0 text-text-secondary" />
      <span className="text-left flex-1 truncate">{getNavItemLabel(item, intl)}</span>
      {item.getTag && (
        <span className="text-xs font-mono text-text-secondary">{item.getTag()}</span>
      )}
    </button>
  );
};

interface SessionRowProps {
  session: Session;
  active: boolean;
  status: SessionStatus | undefined;
  onClick: () => void;
  onRenamed: () => void;
}

const SessionRow: React.FC<SessionRowProps> = ({ session, active, status, onClick, onRenamed }) => {
  const intl = useIntl();
  const [isEditing, setIsEditing] = useState(false);
  const isStreaming = status?.streamState === 'streaming';
  const hasError = status?.streamState === 'error';
  const hasUnread = status?.hasUnreadActivity ?? false;

  return (
    <div
      onClick={() => !isEditing && onClick()}
      className={cn(
        'flex items-center gap-2 px-3 py-1.5 rounded-full cursor-pointer text-sm',
        'hover:bg-background-tertiary/60 transition-colors',
        active && 'bg-background-tertiary'
      )}
    >
      <InlineEditText
        value={getSessionDisplayName(session)}
        onSave={async (newName) => {
          await updateSessionName({
            path: { session_id: session.id },
            body: { name: newName },
          });
          onRenamed();
        }}
        placeholder={intl.formatMessage(i18n.untitledSession)}
        disabled={isStreaming}
        singleClickEdit={false}
        className="truncate text-text-primary flex-1 !px-0 !py-0 hover:bg-transparent"
        editClassName="!text-sm"
        onEditStart={() => setIsEditing(true)}
        onEditEnd={() => setIsEditing(false)}
      />
      <SessionIndicators isStreaming={isStreaming} hasUnread={hasUnread} hasError={hasError} />
    </div>
  );
};

export const Navigation: React.FC<{ className?: string }> = ({ className }) => {
  const intl = useIntl();
  const { isNavExpanded, setIsNavExpanded } = useNavigationContext();
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

  const [isChatsExpanded, setIsChatsExpanded] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');

  const filteredSessions = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return recentSessions;
    return recentSessions.filter((s) => getSessionDisplayName(s).toLowerCase().includes(q));
  }, [recentSessions, searchQuery]);

  if (!isNavExpanded) return null;

  return (
    <motion.div
      ref={navFocusRef}
      tabIndex={-1}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className={cn(
        'bg-background-primary outline-none flex flex-col h-full rounded-lg',
        className
      )}
    >
      {/* Header: logo + collapse button. Top padding clears the macOS traffic lights. */}
      <div className="flex items-center justify-between px-4 pt-[34px] pb-2 no-drag">
        <Goose className="w-6 h-6 text-text-primary" />
        <button
          onClick={() => setIsNavExpanded(false)}
          className="p-1.5 rounded-md hover:bg-background-tertiary transition-colors"
          title={intl.formatMessage(i18n.collapseSidebar)}
        >
          <PanelLeft className="w-4 h-4 text-text-secondary" />
        </button>
      </div>

      {/* Search */}
      <div className="px-3 pb-2">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary pointer-events-none" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={intl.formatMessage(i18n.searchPlaceholder)}
            className={cn(
              'w-full pl-9 pr-3 py-2 rounded-full text-sm bg-background-secondary',
              'border border-transparent focus:border-border-primary focus:bg-background-primary',
              'outline-none transition-colors placeholder:text-text-secondary'
            )}
          />
        </div>
      </div>

      {/* Nav items */}
      <div className="px-2 flex flex-col gap-0.5">
        {visibleItems.map((item) => (
          <NavRow
            key={item.id}
            item={item}
            active={isActive(item.path)}
            onClick={() => handleNavClick(item.path)}
          />
        ))}
      </div>

      {/* Chats section — takes remaining vertical space */}
      <div className="flex-1 min-h-0 flex flex-col mt-3">
        <div className="flex items-center justify-between pr-2">
          <button
            onClick={() => setIsChatsExpanded((v) => !v)}
            className="flex items-center gap-1 px-4 py-1 text-xs font-semibold uppercase tracking-wider text-text-secondary hover:text-text-primary transition-colors"
          >
            {isChatsExpanded ? (
              <ChevronDown className="w-3 h-3" />
            ) : (
              <ChevronRight className="w-3 h-3" />
            )}
            <span>{intl.formatMessage(i18n.chats)}</span>
          </button>
          <button
            onClick={handleNewChat}
            className="p-1 rounded-md hover:bg-background-tertiary transition-colors"
            title={intl.formatMessage(i18n.newChat)}
          >
            <Plus className="w-3.5 h-3.5 text-text-secondary" />
          </button>
        </div>
        {isChatsExpanded && (
          <div className="flex-1 min-h-0 overflow-y-auto px-2 pb-2 mt-1">
            {filteredSessions.length === 0 ? (
              <div className="px-3 py-2 text-xs text-text-secondary">
                {intl.formatMessage(i18n.noChats)}
              </div>
            ) : (
              filteredSessions.map((session) => (
                <SessionRow
                  key={session.id}
                  session={session}
                  active={session.id === activeSessionId}
                  status={sessionStatuses.get(session.id)}
                  onClick={() => {
                    clearUnread(session.id);
                    handleSessionClick(session.id);
                  }}
                  onRenamed={fetchSessions}
                />
              ))
            )}
          </div>
        )}
      </div>

      {/* Settings pinned to bottom */}
      <div className="px-2 pt-2 pb-2 border-t border-border-secondary">
        <NavRow
          item={SETTINGS_NAV_ITEM}
          active={isActive(SETTINGS_NAV_ITEM.path)}
          onClick={() => handleNavClick(SETTINGS_NAV_ITEM.path)}
        />
      </div>
    </motion.div>
  );
};
