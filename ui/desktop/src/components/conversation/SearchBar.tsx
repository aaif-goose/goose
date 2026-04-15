import React, { useEffect, useState, useRef, KeyboardEvent } from 'react';
import { defineMessages, useIntl } from '../../i18n';
import { Search as SearchIcon } from 'lucide-react';
import { ArrowDown, ArrowUp, Close } from '../icons';
import debounce from 'lodash/debounce';
import { Button } from '../ui/button';

const i18nMessages = defineMessages({
  defaultPlaceholder: {
    id: 'searchBar.placeholder',
    defaultMessage: 'Search conversation...',
  },
  caseSensitive: {
    id: 'searchBar.caseSensitive',
    defaultMessage: 'Case Sensitive',
  },
  previous: {
    id: 'searchBar.previous',
    defaultMessage: 'Previous ({shortcut})',
  },
  next: {
    id: 'searchBar.next',
    defaultMessage: 'Next ({shortcut})',
  },
  close: {
    id: 'searchBar.close',
    defaultMessage: 'Close ({shortcut})',
  },
});

/**
 * Props for the SearchBar component
 */
interface SearchBarProps {
  /** Callback fired when search term or case sensitivity changes */
  onSearch: (term: string, caseSensitive: boolean) => void;
  /** Callback fired when the search bar is closed */
  onClose: () => void;
  /** Optional callback for navigating between search results */
  onNavigate?: (direction: 'next' | 'prev') => void;
  /** Current search results state */
  searchResults?: {
    count: number;
    currentIndex: number;
  };
  /** Optional ref for the search input element */
  inputRef?: React.RefObject<HTMLInputElement>;
  /** Initial search term */
  initialSearchTerm?: string;
  /** Placeholder text for the search input */
  placeholder?: string;
}

/**
 * SearchBar provides a search input with case-sensitive toggle and result navigation.
 */
export const SearchBar: React.FC<SearchBarProps> = ({
  onSearch,
  onClose,
  onNavigate,
  searchResults,
  inputRef: externalInputRef,
  initialSearchTerm = '',
  placeholder,
}: SearchBarProps) => {
  const intl = useIntl();
  const resolvedPlaceholder = placeholder ?? intl.formatMessage(i18nMessages.defaultPlaceholder);
  const [searchTerm, setSearchTerm] = useState(initialSearchTerm);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [isExiting, setIsExiting] = useState(false);
  const internalInputRef = React.useRef<HTMLInputElement>(null);
  const inputRef = externalInputRef || internalInputRef;
  const debouncedSearchRef = useRef<ReturnType<typeof debounce> | null>(null);

  // Create debounced search function
  useEffect(() => {
    const debouncedFn = debounce((term: string, caseSensitive: boolean) => {
      onSearch(term, caseSensitive);
    }, 200);

    debouncedSearchRef.current = debouncedFn;

    return () => {
      debouncedFn.cancel();
    };
  }, [onSearch]);

  useEffect(() => {
    inputRef.current?.focus();
  }, [inputRef]);

  // Handle changes to initialSearchTerm
  useEffect(() => {
    if (initialSearchTerm) {
      setSearchTerm(initialSearchTerm);
      if (initialSearchTerm.length >= 2) {
        debouncedSearchRef.current?.(initialSearchTerm, caseSensitive);
      }
    }
  }, [initialSearchTerm, caseSensitive, debouncedSearchRef]);

  const [localSearchResults, setLocalSearchResults] = useState<typeof searchResults>(undefined);

  // Sync external search results with local state
  useEffect(() => {
    // Only set results if we have a search term
    if (!searchTerm) {
      setLocalSearchResults(undefined);
    } else {
      setLocalSearchResults(searchResults);
    }
  }, [searchResults, searchTerm]);

  const handleSearch = (event: React.ChangeEvent<HTMLInputElement>) => {
    const value = event.target.value;

    // Always cancel pending searches first
    if (debouncedSearchRef.current) {
      debouncedSearchRef.current.cancel();
    }

    // Update display term immediately for UI feedback
    setSearchTerm(value);

    // Only trigger search if we have 2 or more characters
    if (value.length >= 2) {
      debouncedSearchRef.current?.(value, caseSensitive);
    } else {
      // Clear results if less than 2 characters
      onSearch('', caseSensitive);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowUp') {
      handleNavigate('prev', event);
    } else if (event.key === 'ArrowDown' || event.key === 'Enter') {
      handleNavigate('next', event);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      handleClose();
    }
  };

  const handleNavigate = (direction: 'next' | 'prev', e?: React.MouseEvent | KeyboardEvent) => {
    e?.preventDefault();
    if (searchResults && searchResults.count > 0) {
      inputRef.current?.focus();
      onNavigate?.(direction);
    }
  };

  const toggleCaseSensitive = () => {
    const newCaseSensitive = !caseSensitive;
    setCaseSensitive(newCaseSensitive);
    // Immediately trigger a new search with updated case sensitivity
    if (searchTerm) {
      debouncedSearchRef.current?.(searchTerm, newCaseSensitive);
    }
    inputRef.current?.focus();
  };

  const handleClose = () => {
    setIsExiting(true);
    debouncedSearchRef.current?.cancel(); // Cancel any pending searches
    setTimeout(() => {
      onClose();
    }, 150); // Match animation duration
  };

  const hasResults = searchResults && searchResults.count > 0;

  return (
    <div
      className={`sticky top-0 bg-background-inverse text-text-inverse z-30 mb-4 ${
        isExiting ? 'search-bar-exit' : 'search-bar-enter'
      }`}
    >
      <div className="flex w-full items-center">
        <div className="relative flex flex-1 items-center h-full min-w-0">
          <SearchIcon className="no-drag h-4 w-4 text-text-inverse/70 absolute left-3" />
          <div className="w-full">
            <input
              ref={inputRef}
              id="search-input"
              type="text"
              value={searchTerm}
              onChange={handleSearch}
              onKeyDown={handleKeyDown}
              placeholder={resolvedPlaceholder}
              className="no-drag w-full text-sm pl-9 pr-24 py-3 bg-background-inverse text-text-inverse
                      placeholder:text-text-inverse/50 focus:outline-none 
                       active:border-border-secondary"
            />
          </div>

          <div className="absolute right-3 flex h-full items-center justify-end">
            <div className="flex items-center gap-1">
              <div className="w-16 text-right text-sm text-text-inverse/80 flex items-center justify-end">
                {(() => {
                  return localSearchResults?.count && localSearchResults.count > 0 && searchTerm
                    ? `${localSearchResults.currentIndex}/${localSearchResults.count}`
                    : null;
                })()}
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-center h-auto px-4 gap-2 flex-shrink-0">
          <Button
            onClick={toggleCaseSensitive}
            variant="ghost"
            className={`no-drag flex items-center justify-center min-w-[32px] h-[28px] rounded transition-all duration-150 ${
              caseSensitive
                ? 'bg-white/20 shadow-[inset_0_1px_2px_rgba(0,0,0,0.2)] text-text-inverse hover:bg-white/25'
                : 'text-text-inverse/70 hover:text-text-inverse hover:bg-white/10'
            }`}
            title={intl.formatMessage(i18nMessages.caseSensitive)}
          >
            <span className="text-md font-normal">Aa</span>
          </Button>

          <div className="flex items-center gap-2">
            <Button
              onClick={(e) => handleNavigate('prev', e)}
              variant="ghost"
              className="no-drag flex items-center justify-center min-w-[32px] h-[28px] rounded transition-all duration-150 text-text-inverse/70 hover:text-text-inverse hover:bg-white/10"
              title={intl.formatMessage(i18nMessages.previous, { shortcut: '↑' })}
            >
              <ArrowUp
                className={`h-5 w-5 transition-opacity ${!hasResults ? 'opacity-30' : ''}`}
              />
            </Button>
            <Button
              onClick={(e) => handleNavigate('next', e)}
              variant="ghost"
              className="no-drag flex items-center justify-center min-w-[32px] h-[28px] rounded transition-all duration-150 text-text-inverse/70 hover:text-text-inverse hover:bg-white/10"
              title={intl.formatMessage(i18nMessages.next, { shortcut: '↓ or Enter' })}
            >
              <ArrowDown
                className={`h-5 w-5 transition-opacity ${!hasResults ? 'opacity-30' : ''}`}
              />
            </Button>
          </div>

          <Button
            onClick={handleClose}
            variant="ghost"
            className="no-drag flex items-center justify-center min-w-[32px] h-[28px] rounded transition-all duration-150 text-text-inverse/70 hover:text-text-inverse hover:bg-white/10"
            title={intl.formatMessage(i18nMessages.close, { shortcut: 'Esc' })}
          >
            <Close className="h-5 w-5" />
          </Button>
        </div>
      </div>
    </div>
  );
};

export default SearchBar;
