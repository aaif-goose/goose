import { useCallback, useRef, useState } from "react";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import { acpSearchSessions } from "@/shared/api/acp";
import {
  buildSessionSearchResults,
  type SessionSearchDisplayResult,
} from "../lib/buildSessionSearchResults";
import type { FilterResolvers } from "../lib/filterSessions";

interface UseSessionSearchOptions {
  sessions: ChatSession[];
  resolvers: FilterResolvers;
  locale?: string;
  getDisplayTitle?: (session: ChatSession) => string;
}

export function useSessionSearch({
  sessions,
  resolvers,
  locale,
  getDisplayTitle,
}: UseSessionSearchOptions) {
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [results, setResults] = useState<SessionSearchDisplayResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestIdRef = useRef(0);

  const clear = useCallback(() => {
    requestIdRef.current += 1;
    setQuery("");
    setSubmittedQuery("");
    setResults([]);
    setIsSearching(false);
    setError(null);
  }, []);

  const updateQuery = useCallback((nextQuery: string) => {
    setQuery(nextQuery);
    if (!nextQuery.trim()) {
      requestIdRef.current += 1;
      setSubmittedQuery("");
      setResults([]);
      setIsSearching(false);
      setError(null);
    }
  }, []);

  const search = useCallback(async () => {
    const trimmed = query.trim();
    if (!trimmed) {
      clear();
      return;
    }

    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;

    const metadataResults = buildSessionSearchResults(
      sessions,
      trimmed,
      [],
      resolvers,
      {
        locale,
        getDisplayTitle,
      },
    );

    setSubmittedQuery(trimmed);
    setError(null);
    setResults(metadataResults);

    const sessionIds = sessions.map((session) => session.id);
    if (trimmed.length < 2 || sessionIds.length === 0) {
      setIsSearching(false);
      return;
    }

    setIsSearching(true);

    try {
      const messageResults = await acpSearchSessions(trimmed, sessionIds);
      if (requestIdRef.current !== requestId) {
        return;
      }

      setResults(
        buildSessionSearchResults(
          sessions,
          trimmed,
          messageResults,
          resolvers,
          {
            locale,
            getDisplayTitle,
          },
        ),
      );
    } catch (searchError) {
      if (requestIdRef.current !== requestId) {
        return;
      }

      const message =
        searchError instanceof Error
          ? searchError.message
          : String(searchError);
      setError(message || "Search failed");
      setResults(metadataResults);
    } finally {
      if (requestIdRef.current === requestId) {
        setIsSearching(false);
      }
    }
  }, [clear, getDisplayTitle, locale, query, resolvers, sessions]);

  return {
    query,
    submittedQuery,
    results,
    isSearching,
    error,
    setQuery: updateQuery,
    search,
    clear,
  };
}
