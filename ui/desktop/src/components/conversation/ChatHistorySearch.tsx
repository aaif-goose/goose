import React, { useState, useCallback, useEffect, useRef } from 'react';
import { Search, X, MessageSquare, ChefHat, Clock } from 'lucide-react';
import { AnimatePresence, motion } from 'framer-motion';
import { defineMessages, useIntl } from '../../i18n';
import { searchSessions } from '../../api';
import { cn } from '../../utils';
import { ScrollArea } from '../ui/scroll-area';
import { Skeleton } from '../ui/skeleton';
import { SessionIndicators } from '../SessionIndicators';
import { getSessionDisplayName, truncateMessage } from '../../hooks/useNavigationSessions';
import type { Session } from '../../api';
import type { SessionStatus } from '../Layout/navigation/types';

const i18n = defineMessages({
  searchPlaceholder: {
    id: 'chatHistorySearch.searchPlaceholder',
    defaultMessage: 'Search chats…',
  },
  noResults: {
    id: 'chatHistorySearch.noResults',
    defaultMessage: 'No chats found',
  },
  searchError: {
    id: 'chatHistorySearch.searchError',
    defaultMessage: 'Search failed',
  },
  messageCount: {
    id: 'chatHistorySearch.messageCount',
    defaultMessage: '{count, plural, one {# message} other {# messages}}',
  },
  keyboardHintMac: {
    id: 'chatHistorySearch.keyboardHintMac',
    defaultMessage: '⌘K',
  },
  keyboardHintOther: {
    id: 'chatHistorySearch.keyboardHintOther',
    defaultMessage: 'Ctrl+K',
  },
});

function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHr = Math.floor(diffMs / 3600000);
  const diffDay = Math.floor(diffMs / 86400000);

  if (diffMin < 1) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHr < 24) return `${diffHr}h ago`;
  if (diffDay < 7) return `${diffDay}d ago`;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

interface ChatHistorySearchProps {
  onSessionClick: (sessionId: string) => void;
  getSessionStatus: (sessionId: string) => SessionStatus | undefined;
  clearUnread: (sessionId: string) => void;
  activeSessionId?: string;
  className?: string;
}

const SEARCH_RESULTS_LIMIT = 8;

export const ChatHistorySearch: React.FC<ChatHistorySearchProps> = ({
  onSessionClick,
  getSessionStatus,
  clearUnread,
  activeSessionId,
  className,
}) => {
  const intl = useIntl();
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Session[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [hasError, setHasError] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const latestRequestIdRef = useRef(0);

  const showDropdown =
    isFocused && (query.trim().length > 0 || isSearching || hasError || results.length > 0);

  const performSearch = useCallback(async (searchQuery: string) => {
    if (!searchQuery.trim()) {
      latestRequestIdRef.current += 1;
      setResults([]);
      setHasError(false);
      return;
    }

    const requestId = ++latestRequestIdRef.current;
    setIsSearching(true);
    setHasError(false);
    try {
      const response = await searchSessions({
        query: { query: searchQuery, limit: SEARCH_RESULTS_LIMIT },
        throwOnError: false,
        client: undefined,
      });

      if (requestId !== latestRequestIdRef.current) return;

      if (response.error || !response.data) {
        setResults([]);
        setHasError(true);
      } else {
        setResults(response.data);
      }
    } catch {
      if (requestId !== latestRequestIdRef.current) return;
      setResults([]);
      setHasError(true);
    } finally {
      if (requestId === latestRequestIdRef.current) {
        setIsSearching(false);
        setHighlightedIndex(-1);
      }
    }
  }, []);

  useEffect(() => {
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    // Invalidate any in-flight request and reset selection so Enter
    // cannot pick a stale result during the debounce window.
    latestRequestIdRef.current += 1;
    setHighlightedIndex(-1);

    if (query.trim()) {
      // Drop previous results and flip to the searching state immediately
      // so (a) an item from the previous query cannot be clicked during the
      // 250ms debounce window and (b) the empty-state does not flicker
      // before the new request actually runs.
      setResults([]);
      setHasError(false);
      setIsSearching(true);
      searchTimeoutRef.current = setTimeout(() => {
        performSearch(query);
      }, 250);
    } else {
      setResults([]);
      setHasError(false);
      setIsSearching(false);
    }

    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [query, performSearch]);

  const handleClear = useCallback(() => {
    setQuery('');
    setResults([]);
    setHasError(false);
    setHighlightedIndex(-1);
    inputRef.current?.focus();
  }, []);

  const handleSelectSession = useCallback(
    (sessionId: string) => {
      clearUnread(sessionId);
      onSessionClick(sessionId);
      setQuery('');
      setResults([]);
      setHighlightedIndex(-1);
      inputRef.current?.blur();
    },
    [onSessionClick, clearUnread]
  );

  const handleContainerBlur = useCallback((event: React.FocusEvent<HTMLDivElement>) => {
    const nextFocusedElement = event.relatedTarget;
    if (!containerRef.current || !nextFocusedElement) {
      setIsFocused(false);
      return;
    }

    if (!containerRef.current.contains(nextFocusedElement)) {
      setIsFocused(false);
    }
  }, []);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsFocused(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  useEffect(() => {
    const isMac = window.electron?.platform === 'darwin';
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((isMac ? e.metaKey : e.ctrlKey) && !e.shiftKey && !e.altKey && e.key === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
        setIsFocused(true);
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleInputKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!showDropdown) return;

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setHighlightedIndex((prev) => (prev < results.length - 1 ? prev + 1 : 0));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setHighlightedIndex((prev) => (prev > 0 ? prev - 1 : results.length - 1));
          break;
        case 'Enter':
          e.preventDefault();
          // Ignore Enter while a search is pending/in-flight so we don't
          // select a result from the previous query.
          if (isSearching) break;
          if (highlightedIndex >= 0 && highlightedIndex < results.length) {
            handleSelectSession(results[highlightedIndex].id);
          }
          break;
        case 'Escape':
          e.preventDefault();
          setIsFocused(false);
          inputRef.current?.blur();
          break;
      }
    },
    [showDropdown, results, highlightedIndex, isSearching, handleSelectSession]
  );

  return (
    <div ref={containerRef} className={cn('relative', className)} onBlur={handleContainerBlur}>
      <div className="relative group">
        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary group-focus-within:text-text-primary transition-colors" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => setIsFocused(true)}
          onKeyDown={handleInputKeyDown}
          placeholder={intl.formatMessage(i18n.searchPlaceholder)}
          aria-label={intl.formatMessage(i18n.searchPlaceholder)}
          aria-expanded={showDropdown}
          aria-autocomplete="list"
          aria-controls="chat-search-results"
          role="combobox"
          className={cn(
            'w-full pl-8 pr-14 py-1.5 text-sm',
            'bg-background-secondary border border-border-primary rounded-lg',
            'text-text-primary placeholder-text-secondary',
            'focus:outline-none focus:ring-1 focus:ring-border-tertiary focus:border-border-tertiary',
            'transition-all duration-150'
          )}
        />
        {query ? (
          <button
            onClick={handleClear}
            className="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded text-text-secondary hover:text-text-primary transition-colors"
            aria-label="Clear search"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        ) : (
          <kbd className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-text-secondary font-mono bg-background-primary px-1.5 py-0.5 rounded border border-border-primary">
            {intl.formatMessage(
              window.electron?.platform === 'darwin' ? i18n.keyboardHintMac : i18n.keyboardHintOther
            )}
          </kbd>
        )}
      </div>

      <AnimatePresence>
        {showDropdown && (
          <motion.div
            id="chat-search-results"
            role="listbox"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.12 }}
            className={cn(
              'absolute z-50 w-full mt-1.5',
              'bg-background-primary border border-border-secondary rounded-lg shadow-lg',
              'overflow-hidden'
            )}
          >
            {isSearching ? (
              <div className="p-2 space-y-2">
                {Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="flex items-center gap-2 px-2 py-1.5">
                    <Skeleton className="w-4 h-4 rounded" />
                    <div className="flex-1 space-y-1">
                      <Skeleton className="h-3 w-3/4 rounded" />
                      <Skeleton className="h-2 w-1/2 rounded" />
                    </div>
                  </div>
                ))}
              </div>
            ) : hasError ? (
              <div className="px-3 py-2.5 text-sm text-red-500">
                {intl.formatMessage(i18n.searchError)}
              </div>
            ) : results.length > 0 ? (
              <ScrollArea className="max-h-72">
                <div className="p-1">
                  {results.map((session, index) => {
                    const status = getSessionStatus(session.id);
                    const isStreaming = status?.streamState === 'streaming';
                    const hasSessionError = status?.streamState === 'error';
                    const hasUnread = status?.hasUnreadActivity ?? false;
                    const isActive = session.id === activeSessionId;
                    const isHighlighted = index === highlightedIndex;
                    const displayName = truncateMessage(getSessionDisplayName(session), 40);
                    const isRecipe = !!session.recipe;

                    return (
                      <button
                        key={session.id}
                        role="option"
                        aria-selected={isHighlighted}
                        onClick={() => handleSelectSession(session.id)}
                        onMouseEnter={() => setHighlightedIndex(index)}
                        className={cn(
                          'w-full flex items-center gap-2 px-2.5 py-2 text-sm rounded-md',
                          'text-left transition-colors duration-75',
                          isHighlighted
                            ? 'bg-background-tertiary'
                            : isActive
                              ? 'bg-background-secondary'
                              : 'hover:bg-background-secondary'
                        )}
                      >
                        {isRecipe ? (
                          <ChefHat className="w-3.5 h-3.5 flex-shrink-0 text-text-secondary" />
                        ) : (
                          <MessageSquare className="w-3.5 h-3.5 flex-shrink-0 text-text-secondary" />
                        )}
                        <div className="flex-1 min-w-0">
                          <div className="font-medium truncate text-text-primary">
                            {displayName}
                          </div>
                          <div className="flex items-center gap-1.5 mt-0.5">
                            {session.message_count > 0 && (
                              <span className="text-[11px] text-text-secondary">
                                {intl.formatMessage(i18n.messageCount, {
                                  count: session.message_count,
                                })}
                              </span>
                            )}
                            <span className="text-[11px] text-text-secondary flex items-center gap-0.5">
                              <Clock className="w-2.5 h-2.5" />
                              {formatRelativeTime(session.updated_at)}
                            </span>
                          </div>
                        </div>
                        <SessionIndicators
                          isStreaming={isStreaming}
                          hasUnread={hasUnread}
                          hasError={hasSessionError}
                        />
                      </button>
                    );
                  })}
                </div>
              </ScrollArea>
            ) : query.trim() ? (
              <div className="px-3 py-2.5 text-sm text-text-secondary">
                {intl.formatMessage(i18n.noResults)}
              </div>
            ) : null}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};
