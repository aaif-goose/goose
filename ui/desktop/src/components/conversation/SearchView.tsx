import React, { useState, useEffect, PropsWithChildren, useCallback, useRef } from 'react';
import SearchBar from './SearchBar';
import { SearchHighlighter } from '../../utils/searchHighlighter';
import debounce from 'lodash/debounce';

/**
 * Props for the SearchView component
 */
interface SearchViewProps {
  /** Optional CSS class name */
  className?: string;
  /** Optional callback for search term changes */
  onSearch?: (term: string, caseSensitive: boolean) => void;
  /** Optional callback for navigating between search results */
  onNavigate?: (direction: 'next' | 'prev') => void;
  /** Current search results state */
  searchResults?: {
    count: number;
    currentIndex: number;
  } | null;
  /** Placeholder text for the search input */
  placeholder?: string;
  /** Show the case-sensitivity toggle (default: true). */
  showCaseSensitive?: boolean;
  /** Show the previous/next match navigation arrows (default: true). */
  showNavigation?: boolean;
  /** Highlight matches in the content via the find-in-page highlighter (default: true).
   * Set false when the search acts as a list filter, so the term only drives onSearch. */
  highlightMatches?: boolean;
}

interface SearchContainerElement extends HTMLDivElement {
  _searchHighlighter: SearchHighlighter | null;
}

const SearchVisibilityContext = React.createContext(false);

export function useSearchVisibility() {
  return React.useContext(SearchVisibilityContext);
}

/**
 * SearchView wraps content in a searchable container with a search bar that appears
 * when Cmd/Ctrl+F is pressed. Supports case-sensitive search and result navigation.
 * Features debounced search for better performance with large content.
 */
export const SearchView: React.FC<PropsWithChildren<SearchViewProps>> = ({
  className = '',
  children,
  onSearch,
  onNavigate,
  searchResults,
  placeholder,
  showCaseSensitive = true,
  showNavigation = true,
  highlightMatches = true,
}) => {
  const [isSearchVisible, setIsSearchVisible] = useState(false);
  const [initialSearchTerm, setInitialSearchTerm] = useState('');
  const [internalSearchResults, setInternalSearchResults] = useState<{
    currentIndex: number;
    count: number;
  } | null>(null);

  const searchInputRef: React.RefObject<HTMLInputElement> = React.useRef<HTMLInputElement>(
    null
  ) as React.RefObject<HTMLInputElement>;
  const highlighterRef = React.useRef<SearchHighlighter | null>(null);
  const containerRef = React.useRef<SearchContainerElement | null>(null);
  const lastSearchRef = React.useRef<{ term: string; caseSensitive: boolean }>({
    term: '',
    caseSensitive: false,
  });

  // Create debounced highlight function
  const debouncedHighlight = useCallback(
    (term: string, caseSensitive: boolean, highlighter: SearchHighlighter) => {
      const performHighlight = () => {
        const highlights = highlighter.highlight(term, caseSensitive);
        const count = highlights.length;

        if (count > 0) {
          setInternalSearchResults({
            currentIndex: 1,
            count,
          });
          highlighter.setCurrentMatch(0, true);
        } else {
          setInternalSearchResults(null);
        }
      };

      // If this is a case sensitivity change (same term, different case setting),
      // execute immediately
      if (
        term === lastSearchRef.current.term &&
        caseSensitive !== lastSearchRef.current.caseSensitive
      ) {
        performHighlight();
        return;
      }

      // Create a debounced version of performHighlight
      const debouncedFn = debounce(performHighlight, 150);
      debouncedFn();

      // Store the debounced function for potential cancellation
      return debouncedFn;
    },
    []
  );

  /**
   * Handles the search operation when a user enters a search term.
   * Uses debouncing to prevent excessive highlighting operations.
   * @param term - The text to search for
   * @param caseSensitive - Whether to perform a case-sensitive search
   */
  const handleSearch = useCallback(
    (term: string, caseSensitive: boolean) => {
      // Store the latest search parameters
      const isCaseChange =
        term === lastSearchRef.current.term &&
        caseSensitive !== lastSearchRef.current.caseSensitive;

      lastSearchRef.current = { term, caseSensitive };

      // Call the onSearch callback if provided
      onSearch?.(term, caseSensitive);

      // When highlighting is disabled (search acts as a list filter), the term only drives
      // onSearch — skip all find-in-page highlighter machinery.
      if (!highlightMatches) {
        return;
      }

      // If empty, clear everything and return
      if (!term) {
        setInternalSearchResults(null);
        if (highlighterRef.current) {
          highlighterRef.current.clearHighlights();
        }
        return;
      }

      const container = containerRef.current;
      if (!container) return;

      // For case sensitivity changes, reuse existing highlighter
      if (isCaseChange && highlighterRef.current) {
        debouncedHighlight(term, caseSensitive, highlighterRef.current);
        return;
      }

      // Otherwise create new highlighter
      if (highlighterRef.current) {
        highlighterRef.current.clearHighlights();
        highlighterRef.current.destroy();
      }

      highlighterRef.current = new SearchHighlighter(container, (count) => {
        // Only update if this is still the latest search
        if (
          lastSearchRef.current.term === term &&
          lastSearchRef.current.caseSensitive === caseSensitive
        ) {
          if (count > 0) {
            setInternalSearchResults({
              currentIndex: 1,
              count,
            });
          } else {
            setInternalSearchResults(null);
          }
        }
      });

      debouncedHighlight(term, caseSensitive, highlighterRef.current);
    },
    [debouncedHighlight, onSearch, highlightMatches]
  );

  /**
   * Navigates between search results in the specified direction.
   * @param direction - Direction to navigate ('next' or 'prev')
   */
  const handleNavigate = useCallback(
    (direction: 'next' | 'prev') => {
      // If external navigation is provided, use that
      if (onNavigate) {
        onNavigate(direction);
        return;
      }

      // Otherwise use internal navigation
      if (!internalSearchResults || !highlighterRef.current) return;

      let newIndex: number;
      if (direction === 'next') {
        newIndex = (internalSearchResults.currentIndex % internalSearchResults.count) + 1;
      } else {
        newIndex =
          internalSearchResults.currentIndex === 1
            ? internalSearchResults.count
            : internalSearchResults.currentIndex - 1;
      }

      setInternalSearchResults({
        ...internalSearchResults,
        currentIndex: newIndex,
      });

      highlighterRef.current.setCurrentMatch(newIndex - 1, true);
    },
    [internalSearchResults, onNavigate]
  );

  // Create stable refs for the handlers to avoid memory leaks
  const handlersRef = useRef({
    handleFindCommand: () => {
      if (isSearchVisible && searchInputRef.current) {
        searchInputRef.current.focus();
        searchInputRef.current.select();
      } else {
        setIsSearchVisible(true);
      }
    },
    handleFindNext: () => {
      if (isSearchVisible) {
        handleNavigate('next');
      }
    },
    handleFindPrevious: () => {
      if (isSearchVisible) {
        handleNavigate('prev');
      }
    },
    handleUseSelectionFind: () => {
      const selection = window.getSelection()?.toString().trim();
      if (selection) {
        setInitialSearchTerm(selection);
      }
    },
  });

  // Update the refs with current values
  useEffect(() => {
    handlersRef.current.handleFindCommand = () => {
      if (isSearchVisible && searchInputRef.current) {
        searchInputRef.current.focus();
        searchInputRef.current.select();
      } else {
        setIsSearchVisible(true);
      }
    };
    handlersRef.current.handleFindNext = () => {
      if (isSearchVisible) {
        handleNavigate('next');
      }
    };
    handlersRef.current.handleFindPrevious = () => {
      if (isSearchVisible) {
        handleNavigate('prev');
      }
    };
    handlersRef.current.handleUseSelectionFind = () => {
      const selection = window.getSelection()?.toString().trim();
      if (selection) {
        setInitialSearchTerm(selection);
      }
    };
  });

  const handleFindCommand = useCallback(() => {
    handlersRef.current.handleFindCommand();
  }, []);

  const handleFindNext = useCallback(() => {
    handlersRef.current.handleFindNext();
  }, []);

  const handleFindPrevious = useCallback(() => {
    handlersRef.current.handleFindPrevious();
  }, []);

  const handleUseSelectionFind = useCallback(() => {
    handlersRef.current.handleUseSelectionFind();
  }, []);

  /**
   * Closes the search interface and cleans up highlights.
   */
  const handleCloseSearch = useCallback(() => {
    setIsSearchVisible(false);
    setInternalSearchResults(null);
    lastSearchRef.current = { term: '', caseSensitive: false };

    if (highlighterRef.current) {
      highlighterRef.current.clearHighlights();
      highlighterRef.current.destroy();
      highlighterRef.current = null;
    }

    // Clear search when closing
    onSearch?.('', false);
  }, [onSearch]);

  // Clean up highlighter on unmount
  useEffect(() => {
    return () => {
      if (highlighterRef.current) {
        highlighterRef.current.destroy();
        highlighterRef.current = null;
      }
    };
  }, []);

  // Listen for keyboard events
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const isMac = window.electron.platform === 'darwin';

      // Handle ⌘F/Ctrl+F to show/focus search
      if ((isMac ? e.metaKey : e.ctrlKey) && !e.shiftKey && e.key === 'f') {
        e.preventDefault();
        if (isSearchVisible && searchInputRef.current) {
          // If search is already visible, focus and select the input
          searchInputRef.current.focus();
          searchInputRef.current.select();
        } else {
          // Otherwise show the search UI
          setIsSearchVisible(true);
        }
        return;
      }

      // Handle ⌘E to use selection for find (Mac only)
      if (isMac && e.metaKey && !e.shiftKey && e.key === 'e') {
        // Don't handle ⌘E if we're in the search input - let the native behavior work
        if (e.target instanceof HTMLInputElement && e.target.id === 'search-input') {
          return;
        }

        e.preventDefault();
        handleUseSelectionFind();
        return;
      }

      // Only handle ⌘G and ⇧⌘G if search is visible (Mac only)
      if (isSearchVisible && isMac && e.metaKey && e.key === 'g') {
        e.preventDefault();
        if (e.shiftKey) {
          // ⇧⌘G - Find Previous
          handleNavigate('prev');
        } else {
          // ⌘G - Find Next
          handleNavigate('next');
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [isSearchVisible, handleNavigate, handleSearch, handleUseSelectionFind]);

  // Listen for Find menu commands
  useEffect(() => {
    window.electron.on('find-command', handleFindCommand);
    window.electron.on('find-next', handleFindNext);
    window.electron.on('find-previous', handleFindPrevious);
    window.electron.on('use-selection-find', handleUseSelectionFind);

    return () => {
      window.electron.off('find-command', handleFindCommand);
      window.electron.off('find-next', handleFindNext);
      window.electron.off('find-previous', handleFindPrevious);
      window.electron.off('use-selection-find', handleUseSelectionFind);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Empty dependency array - handlers are stable due to useCallback and useRef

  return (
    <div
      ref={(el) => {
        if (el) {
          containerRef.current = el as SearchContainerElement;
          // Expose the highlighter instance
          containerRef.current._searchHighlighter = highlighterRef.current;
        }
      }}
      className={`search-container ${className}`}
    >
      {isSearchVisible && (
        <SearchBar
          onSearch={handleSearch}
          onClose={handleCloseSearch}
          onNavigate={handleNavigate}
          searchResults={searchResults || internalSearchResults || undefined}
          inputRef={searchInputRef}
          initialSearchTerm={initialSearchTerm}
          placeholder={placeholder}
          showCaseSensitive={showCaseSensitive}
          showNavigation={showNavigation}
        />
      )}
      <SearchVisibilityContext.Provider value={isSearchVisible}>
        {children}
      </SearchVisibilityContext.Provider>
    </div>
  );
};
