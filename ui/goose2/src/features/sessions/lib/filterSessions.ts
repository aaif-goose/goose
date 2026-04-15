import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import { formatDate } from "@/shared/i18n";

export interface FilterResolvers {
  getPersonaName: (personaId: string) => string | undefined;
  getProjectName: (projectId: string) => string | undefined;
}

interface FilterOptions {
  locale?: string;
  getDisplayTitle?: (session: ChatSession) => string;
}

function buildSearchableString(
  session: ChatSession,
  resolvers: FilterResolvers,
  options: FilterOptions,
): string {
  const displayTitle = options.getDisplayTitle?.(session) ?? session.title;
  const parts: string[] = [displayTitle];

  if (displayTitle !== session.title) {
    parts.push(session.title);
  }

  if (session.personaId) {
    const name = resolvers.getPersonaName(session.personaId);
    if (name) parts.push(name);
  }

  if (session.projectId) {
    const name = resolvers.getProjectName(session.projectId);
    if (name) parts.push(name);
  }

  const date = new Date(session.updatedAt);
  parts.push(
    formatDate(
      date,
      {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric",
      },
      options.locale,
    ),
  );

  return parts.join(" ").toLowerCase();
}

export function filterSessions(
  sessions: ChatSession[],
  query: string,
  resolvers: FilterResolvers,
  options: FilterOptions = {},
): ChatSession[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return sessions;

  return sessions.filter((session) =>
    buildSearchableString(session, resolvers, options).includes(trimmed),
  );
}
