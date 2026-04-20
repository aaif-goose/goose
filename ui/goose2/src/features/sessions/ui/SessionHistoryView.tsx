import { useCallback, useMemo, useRef } from "react";
import { History, Upload } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";
import { SearchBar } from "@/shared/ui/SearchBar";
import { Button } from "@/shared/ui/button";
import { SessionCard } from "./SessionCard";
import { groupSessionsByDate } from "../lib/groupSessionsByDate";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import {
  getVisibleSessions,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import {
  acpDuplicateSession,
  acpExportSession,
  acpImportSession,
} from "@/shared/api/acp";
import { saveExportedSessionFile } from "@/shared/api/system";
import { defaultExportFilename, downloadJson } from "../lib/exportSession";
import { useSessionSearch } from "../hooks/useSessionSearch";

interface SessionHistoryViewProps {
  onSelectSession?: (sessionId: string) => void;
  onSelectSearchResult?: (
    sessionId: string,
    messageId?: string,
    query?: string,
  ) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onArchiveChat?: (sessionId: string) => void;
}

export function SessionHistoryView({
  onSelectSession,
  onSelectSearchResult,
  onRenameChat,
  onArchiveChat,
}: SessionHistoryViewProps) {
  const { t, i18n } = useTranslation(["sessions", "common"]);
  const sessions = useChatSessionStore((s) => s.sessions);
  const messagesBySession = useChatStore((s) => s.messagesBySession);
  const loadSessions = useChatSessionStore((s) => s.loadSessions);
  const activeSessions = useMemo(
    () =>
      getVisibleSessions(sessions, messagesBySession).filter(
        (session) => !session.archivedAt,
      ),
    [messagesBySession, sessions],
  );
  const fileInputRef = useRef<HTMLInputElement>(null);

  const getPersonaName = useCallback(
    (personaId: string) =>
      useAgentStore.getState().getPersonaById(personaId)?.displayName,
    [],
  );

  const projects = useProjectStore((s) => s.projects);
  const getProjectName = useCallback(
    (projectId: string) => projects.find((p) => p.id === projectId)?.name,
    [projects],
  );

  const getProjectColor = useCallback(
    (projectId: string) => projects.find((p) => p.id === projectId)?.color,
    [projects],
  );

  const getWorkingDir = useCallback(
    (projectId: string) =>
      projects.find((p) => p.id === projectId)?.workingDirs[0],
    [projects],
  );

  const resolvers = { getPersonaName, getProjectName };
  const search = useSessionSearch({
    sessions: activeSessions,
    resolvers,
    locale: i18n.resolvedLanguage,
    getDisplayTitle: (session) =>
      getDisplaySessionTitle(session.title, t("common:session.defaultTitle")),
  });
  const dateGroups = groupSessionsByDate(activeSessions, {
    locale: i18n.resolvedLanguage,
    todayLabel: t("dateGroups.today"),
    yesterdayLabel: t("dateGroups.yesterday"),
  });

  const handleArchive = useCallback(
    async (sessionId: string) => {
      if (onArchiveChat) {
        await onArchiveChat(sessionId);
        return;
      }

      try {
        await useChatSessionStore.getState().archiveSession(sessionId);
      } catch {
        // best-effort
      }
    },
    [onArchiveChat],
  );

  const handleExport = useCallback(
    async (sessionId: string) => {
      try {
        const session = activeSessions.find((s) => s.id === sessionId);
        const json = await acpExportSession(sessionId);
        const filename = defaultExportFilename(session?.title ?? "session");

        if (window.__TAURI_INTERNALS__) {
          const savedPath = await saveExportedSessionFile(filename, json);
          if (!savedPath) {
            return;
          }
          toast.success(`Exported session to ${filename}`);
          return;
        }

        downloadJson(json, filename);
        toast.success(`Exported session to ${filename}`);
      } catch (error) {
        console.error("Export failed:", error);
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("not found in sessions or threads")) {
          await loadSessions();
        }
        toast.error("Failed to export session");
      }
    },
    [activeSessions, loadSessions],
  );

  const handleDuplicate = useCallback(
    async (sessionId: string) => {
      try {
        await acpDuplicateSession(sessionId);
        await loadSessions();
      } catch (error) {
        console.error("Duplicate failed:", error);
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("not found in sessions or threads")) {
          await loadSessions();
        }
      }
    },
    [loadSessions],
  );

  const handleImportSession = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;

      try {
        const text = await file.text();
        await acpImportSession(text);
        await loadSessions();
      } catch (error) {
        console.error("Import failed:", error);
      } finally {
        if (fileInputRef.current) {
          fileInputRef.current.value = "";
        }
      }
    },
    [loadSessions],
  );

  const handleSelectResult = useCallback(
    (sessionId: string, messageId?: string) => {
      if (messageId) {
        onSelectSearchResult?.(sessionId, messageId, search.submittedQuery);
        return;
      }
      onSelectSession?.(sessionId);
    },
    [onSelectSearchResult, onSelectSession, search.submittedQuery],
  );

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="page-transition mx-auto flex w-full max-w-5xl flex-col gap-5 px-6 py-8">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h1 className="font-display text-lg font-semibold tracking-tight">
                {t("history.title")}
              </h1>
              <p className="text-xs text-muted-foreground">
                {t("history.subtitle")}
              </p>
            </div>
            <Button
              type="button"
              variant="outline-flat"
              size="xs"
              onClick={() => fileInputRef.current?.click()}
            >
              <Upload className="size-3.5" />
              {t("common:actions.import")}
            </Button>
          </div>

          <SearchBar
            value={search.query}
            onChange={search.setQuery}
            placeholder={t("history.searchPlaceholder")}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void search.search();
              }
            }}
          />

          {search.error && (
            <p className="text-xs text-danger">{t("history.searchError")}</p>
          )}

          {search.submittedQuery ? (
            search.results.length > 0 ? (
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {search.results.map((result) => (
                  <SessionCard
                    key={result.session.id}
                    id={result.session.id}
                    title={result.session.title}
                    updatedAt={result.session.updatedAt}
                    personaName={
                      result.session.personaId
                        ? getPersonaName(result.session.personaId)
                        : undefined
                    }
                    projectName={
                      result.session.projectId
                        ? getProjectName(result.session.projectId)
                        : undefined
                    }
                    projectColor={
                      result.session.projectId
                        ? getProjectColor(result.session.projectId)
                        : undefined
                    }
                    workingDir={
                      result.session.projectId
                        ? getWorkingDir(result.session.projectId)
                        : undefined
                    }
                    archivedAt={result.session.archivedAt}
                    snippet={result.snippet}
                    matchCount={result.matchCount}
                    onSelect={() =>
                      handleSelectResult(result.session.id, result.messageId)
                    }
                    onRename={onRenameChat}
                    onArchive={handleArchive}
                    onExport={handleExport}
                    onDuplicate={handleDuplicate}
                  />
                ))}
              </div>
            ) : (
              <div className="flex flex-col items-center justify-center gap-3 py-16 text-muted-foreground">
                <History className="h-10 w-10 opacity-30" />
                <div className="text-center">
                  <p className="text-sm font-medium">
                    {search.isSearching
                      ? t("history.searching")
                      : t("history.emptyNoMatches")}
                  </p>
                  {!search.isSearching && (
                    <p className="mt-1 text-xs text-muted-foreground">
                      {t("history.emptyNoMatchesHint")}
                    </p>
                  )}
                </div>
              </div>
            )
          ) : dateGroups.length > 0 ? (
            dateGroups.map((group) => (
              <div key={group.label} className="space-y-2">
                <h2 className="sticky top-0 z-10 bg-background py-1 text-sm font-medium text-muted-foreground">
                  {group.label}
                </h2>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                  {group.sessions.map((session) => (
                    <SessionCard
                      key={session.id}
                      id={session.id}
                      title={session.title}
                      updatedAt={session.updatedAt}
                      personaName={
                        session.personaId
                          ? getPersonaName(session.personaId)
                          : undefined
                      }
                      projectName={
                        session.projectId
                          ? getProjectName(session.projectId)
                          : undefined
                      }
                      projectColor={
                        session.projectId
                          ? getProjectColor(session.projectId)
                          : undefined
                      }
                      workingDir={
                        session.projectId
                          ? getWorkingDir(session.projectId)
                          : undefined
                      }
                      archivedAt={session.archivedAt}
                      onSelect={onSelectSession}
                      onRename={onRenameChat}
                      onArchive={handleArchive}
                      onExport={handleExport}
                      onDuplicate={handleDuplicate}
                    />
                  ))}
                </div>
              </div>
            ))
          ) : (
            <div className="flex flex-col items-center justify-center gap-3 py-16 text-muted-foreground">
              <History className="h-10 w-10 opacity-30" />
              <div className="text-center">
                <p className="text-sm font-medium">{t("history.emptyTitle")}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("history.emptyHint")}
                </p>
              </div>
            </div>
          )}
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        accept=".json"
        onChange={handleImportSession}
        className="hidden"
      />
    </div>
  );
}
