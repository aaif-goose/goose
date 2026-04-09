import React, { useState, useCallback, useEffect, useRef } from 'react';
import { Search, X, Loader2 } from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { searchSessions } from '../../api';
import { cn } from '../../utils';
import { SessionIndicators } from '../SessionIndicators';
import type { Session } from '../../api';
import type { SessionStatus } from '../Layout/navigation/types';

const i18n = defineMessages({
  searchPlaceholder: {
    id: 'chatHistorySearch.searchPlaceholder',
    defaultMessage: 'Search chat history...',
  },
  noResults: {
    id: 'chatHistorySearch.noResults',
    defaultMessage: 'No results found',
  },
  searching: {
    id: 'chatHistorySearch.searching',
    defaultMessage: 'Searching...',
  },
});

interface ChatHistorySearchProps {
  onSessionClick: (sessionId: string) => void;
  getSessionStatus: (sessionId: string) => SessionStatus | undefined;
  clearUnread: (sessionId: string) => void;
  activeSessionId?: string;
  className?: string;
}

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
  const [showResults, setShowResults] = useState(false);
  const searchRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const performSearch = useCallback(async (searchQuery: string) => {
    if (!searchQuery.trim()) {
      setResults([]);
      setShowResults(false);
      return;
    }

    setIsSearching(true);
    try {
      const response = await searchSessions({
        query: { query: searchQuery, limit: 10 },
        throwOnError: false,
        client: undefined,
      });

      if (response.data) {
        setResults(response.data);
        setShowResults(true);
      }
    } catch (error) {
      console.error('Search failed:', error);
      setResults([]);
    } finally {
      setIsSearching(false);
    }
  }, []);

  // Debounced search
  useEffect(() => {
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    if (query.trim()) {
      searchTimeoutRef.current = setTimeout(() => {
        performSearch(query);
      }, 300);
    } else {
      setResults([]);
      setShowResults(false);
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
    setShowResults(false);
    inputRef.current?.focus();
  }, []);

  const handleResultClick = useCallback(
    (sessionId: string) => {
      clearUnread(sessionId);
      onSessionClick(sessionId);
      setShowResults(false);
      setQuery('');
    },
    [onSessionClick, clearUnread]
  );

  // Close results when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (searchRef.current && !searchRef.current.contains(event.target as Node)) {
        setShowResults(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Focus input on mount and when search opens
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd/Ctrl + K to focus search
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
      }
      // Escape to clear and close
      if (e.key === 'Escape' && showResults) {
        e.preventDefault();
        setShowResults(false);
        if (!query) {
          inputRef.current?.blur();
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [showResults, query]);

  return (
    <div ref={searchRef} className={cn('relative', className)}>
      {/* Search Input */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => {
            if (results.length > 0) setShowResults(true);
          }}
          placeholder={intl.formatMessage(i18n.searchPlaceholder)}
          className={cn(
            'w-full pl-10 pr-10 py-2 text-sm',
            'bg-background-secondary border border-border-primary rounded-lg',
            'text-text-primary placeholder-text-secondary',
            'focus:outline-none focus:ring-2 focus:ring-border-tertiary',
            'transition-all duration-200'
          )}
        />
        {query && (
          <button
            onClick={handleClear}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* Search Results Dropdown */}
      {showResults && (
        <div
          className={cn(
            'absolute z-50 w-full mt-2',
            'bg-background-primary border border-border-secondary rounded-lg shadow-lg',
            'max-h-96 overflow-y-auto'
          )}
        >
          {isSearching ? (
            <div className="flex items-center gap-2 px-4 py-3 text-sm text-text-secondary">
              <Loader2 className="w-4 h-4 animate-spin" />
              <span>{intl.formatMessage(i18n.searching)}</span>
            </div>
          ) : results.length > 0 ? (
            <div className="p-1">
              {results.map((session) => {
                const status = getSessionStatus(session.id);
                const isStreaming = status?.streamState === 'streaming';
                const hasError = status?.streamState === 'error';
                const hasUnread = status?.hasUnreadActivity ?? false;
                const isActiveSession = session.id === activeSessionId;

                return (
                  <button
                    key={session.id}
                    onClick={() => handleResultClick(session.id)}
                    className={cn(
                      'w-full flex items-center gap-2 px-3 py-2 text-sm rounded-lg',
                      'text-left transition-colors duration-150',
                      isActiveSession
                        ? 'bg-background-tertiary text-text-primary'
                        : 'hover:bg-background-secondary text-text-primary'
                    )}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="font-medium truncate">{session.name}</div>
                      {session.message_count > 0 && (
                        <div className="text-xs text-text-secondary mt-0.5">
                          {session.message_count}{' '}
                          {session.message_count === 1 ? 'message' : 'messages'}
                        </div>
                      )}
                    </div>
                    <SessionIndicators
                      isStreaming={isStreaming}
                      hasUnread={hasUnread}
                      hasError={hasError}
                    />
                  </button>
                );
              })}
            </div>
          ) : query.trim() ? (
            <div className="px-4 py-3 text-sm text-text-secondary">
              {intl.formatMessage(i18n.noResults)}
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
};
