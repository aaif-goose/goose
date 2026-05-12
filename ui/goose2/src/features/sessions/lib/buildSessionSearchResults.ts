import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import type { AcpSessionSearchResult } from "@/shared/api/acp";
import { filterSessions, type FilterResolvers } from "./filterSessions";

interface BuildSessionSearchResultsOptions {
  locale?: string;
  getDisplayTitle?: (session: ChatSession) => string;
}

export interface SessionSearchDisplayResult {
  session: ChatSession;
  matchType: "metadata" | "message";
  snippet?: string;
  messageId?: string;
  messageRole?: "user" | "assistant" | "system";
  matchCount?: number;
}

function sortByUpdatedAtDesc(sessions: ChatSession[]): ChatSession[] {
  return [...sessions].sort(
    (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
  );
}

export function buildSessionSearchResults(
  sessions: ChatSession[],
  query: string,
  messageMatches: AcpSessionSearchResult[],
  resolvers: FilterResolvers,
  options: BuildSessionSearchResultsOptions = {},
): SessionSearchDisplayResult[] {
  const metadataMatchIds = new Set(
    filterSessions(sessions, query, resolvers, options).map(
      (session) => session.id,
    ),
  );
  const messageMatchesBySessionId = new Map(
    messageMatches.map((match) => [match.sessionId, match]),
  );

  return sortByUpdatedAtDesc(sessions)
    .filter((session) => {
      return (
        metadataMatchIds.has(session.id) ||
        messageMatchesBySessionId.has(session.id)
      );
    })
    .map((session) => {
      const messageMatch = messageMatchesBySessionId.get(session.id);
      if (!messageMatch) {
        return {
          session,
          matchType: "metadata" as const,
        };
      }

      return {
        session,
        matchType: "message" as const,
        snippet: messageMatch.snippet,
        messageId: messageMatch.messageId,
        messageRole: messageMatch.messageRole,
        matchCount: messageMatch.matchCount,
      };
    });
}
