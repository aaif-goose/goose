import { HomeScreen } from "@/features/home/ui/HomeScreen";
import { HomeView } from "@/features/home/ui/HomeView";
import { ChatView } from "@/features/chat/ui/ChatView";
import { SkillsView } from "@/features/skills/ui/SkillsView";
import { AgentsView } from "@/features/agents/ui/AgentsView";
import { ProjectsView } from "@/features/projects/ui/ProjectsView";
import { SessionHistoryView } from "@/features/sessions/ui/SessionHistoryView";
import { SearchView } from "@/features/search/ui/SearchView";
import type { ExtensionEntry } from "@/features/extensions/types";
import type { SkillInfo } from "@/features/skills/api/skills";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import type { ProjectInfo } from "@/features/projects/api/projects";
import type { AppView } from "../types";

interface AppShellContentProps {
  activeView: AppView;
  activeSession?: ChatSession;
  homeSessionId: string | null;
  onCreatePersona: () => void;
  onArchiveChat: (sessionId: string) => Promise<void>;
  onCreateProject: (options?: {
    initialWorkingDir?: string | null;
    onCreated?: (projectId: string) => void;
  }) => void;
  onActivateHomeSession: (sessionId: string) => void;
  onRenameChat: (sessionId: string, nextTitle: string) => void;
  onSelectSession: (sessionId: string) => void;
  onSelectSearchResult: (
    sessionId: string,
    messageId?: string,
    query?: string,
  ) => void;
  onExitSearch: () => void;
  onOpenExtension: (entry: ExtensionEntry) => void;
  onOpenAgent: (agentId: string) => void;
  onStartChatWithPersona: (personaId: string) => void;
  onOpenSkill: (skill: SkillInfo) => void;
  onStartChatFromProject: (project: ProjectInfo) => void;
  openAgentId?: string | null;
  openSkill?: SkillInfo | null;
  onOpenAgentConsumed?: () => void;
  onOpenSkillConsumed?: () => void;
}

export function AppShellContent({
  activeView,
  activeSession,
  homeSessionId,
  onCreatePersona,
  onArchiveChat,
  onCreateProject,
  onActivateHomeSession,
  onRenameChat,
  onSelectSession,
  onSelectSearchResult,
  onExitSearch,
  onOpenExtension,
  onOpenAgent,
  onStartChatWithPersona,
  onOpenSkill,
  onStartChatFromProject,
  openAgentId,
  openSkill,
  onOpenAgentConsumed,
  onOpenSkillConsumed,
}: AppShellContentProps) {
  switch (activeView) {
    case "skills":
      return (
        <SkillsView
          openSkill={openSkill}
          onOpenSkillConsumed={onOpenSkillConsumed}
        />
      );
    case "agents":
      return (
        <AgentsView
          openAgentId={openAgentId}
          onOpenAgentConsumed={onOpenAgentConsumed}
        />
      );
    case "projects":
      return <ProjectsView onStartChat={onStartChatFromProject} />;
    case "session-history":
      return (
        <SessionHistoryView
          onSelectSession={onSelectSession}
          onSelectSearchResult={onSelectSearchResult}
          onRenameChat={onRenameChat}
          onArchiveChat={onArchiveChat}
        />
      );
    case "chat":
      return activeSession ? (
        <ChatView
          key={activeSession.id}
          sessionId={activeSession.id}
          onCreatePersona={onCreatePersona}
          onCreateProject={onCreateProject}
        />
      ) : (
        <HomeScreen
          sessionId={homeSessionId}
          onActivateSession={onActivateHomeSession}
          onCreatePersona={onCreatePersona}
          onCreateProject={onCreateProject}
        />
      );
    case "home":
      return (
        <HomeView
          onOpenAgent={onOpenAgent}
          onStartChatWithPersona={onStartChatWithPersona}
          onSelectSession={onSelectSession}
        />
      );
    case "search":
      return (
        <SearchView
          onExit={onExitSearch}
          onSelectSearchResult={onSelectSearchResult}
          onOpenExtension={onOpenExtension}
          onOpenAgent={onOpenAgent}
          onOpenSkill={onOpenSkill}
        />
      );
  }
}
