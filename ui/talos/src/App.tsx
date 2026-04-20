import { useEffect, useState } from "react";
import { LeftFooter, LeftHeader, Ribbon } from "./components/sidebar/Ribbon";
import {
  ChatSection,
  MemorySection,
  ProjectsSection,
  SectionSwitcher,
  SkillsSection,
  WorkflowsSection,
} from "./components/sidebar/Sections";
import { TabBar } from "./components/chat/TabBar";
import { EmptyState } from "./components/chat/EmptyState";
import { ChatView } from "./components/chat/ChatView";
import type { ComposerProps } from "./components/chat/Composer";
import { RightPanel } from "./components/right/RightPanel";
import { CommandPalette } from "./components/palette/CommandPalette";
import { StatusBar } from "./components/StatusBar";
import { ToastStack } from "./components/Toast";
import { CHATS, MCP_SERVERS, MODELS, PROJECTS, SECTIONS, SKILLS, USER } from "./data";
import type { ChatTab, Command, McpServer, SectionId, Skill, Toast } from "./types";
import { generateFakeConversation, makeCannedAssistantReply, truncate } from "./mockReply";

export function App() {
  const [activeApp, setActiveApp] = useState("chat");
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [section, setSection] = useState<SectionId>("chat");
  const [search, setSearch] = useState("");
  const [tabs, setTabs] = useState<ChatTab[]>([
    { id: "t1", title: "New chat", chatId: null, messages: [], composer: "", attachments: [] },
  ]);
  const [activeTab, setActiveTab] = useState("t1");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [openNote, setOpenNote] = useState<string | null>(null);
  const [model, setModel] = useState("opus-4.7");
  const [mcpServers, setMcpServers] = useState<McpServer[]>(MCP_SERVERS);
  const [contextFolder, setContextFolder] = useState("memory");
  const [skills, setSkills] = useState<Skill[]>(SKILLS);
  const [sending, setSending] = useState(false);

  const addToast = (msg: string) => {
    const id = Date.now() + Math.random();
    setToasts((ts) => [...ts, { id, msg }]);
    setTimeout(() => setToasts((ts) => ts.filter((t) => t.id !== id)), 2400);
  };

  const newTab = () => {
    const id = "t" + Date.now();
    setTabs((ts) => [...ts, { id, title: "New chat", chatId: null, messages: [], composer: "", attachments: [] }]);
    setActiveTab(id);
  };

  const closeTab = (id: string) => {
    const idx = tabs.findIndex((t) => t.id === id);
    const next = tabs.filter((t) => t.id !== id);
    if (next.length === 0) {
      newTab();
      return;
    }
    setTabs(next);
    if (activeTab === id) setActiveTab(next[Math.max(0, idx - 1)]!.id);
  };

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((p) => !p);
      } else if (meta && e.key.toLowerCase() === "n") {
        e.preventDefault();
        newTab();
      } else if (meta && e.key.toLowerCase() === "b" && !e.shiftKey) {
        e.preventDefault();
        setLeftCollapsed((c) => !c);
      } else if (meta && e.shiftKey && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setRightCollapsed((c) => !c);
      } else if (meta && e.key === "/") {
        e.preventDefault();
        (document.querySelector(".composer-input") as HTMLTextAreaElement | null)?.focus();
      } else if (meta && e.key >= "1" && e.key <= "5") {
        e.preventDefault();
        const s = SECTIONS[parseInt(e.key, 10) - 1];
        if (s) setSection(s.id);
      } else if (e.key === "Escape") {
        setPaletteOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const openChatSession = (chatId: string) => {
    const existing = tabs.find((t) => t.chatId === chatId);
    if (existing) {
      setActiveTab(existing.id);
      return;
    }
    const chat = CHATS.find((c) => c.id === chatId);
    if (!chat) return;
    const msgs = generateFakeConversation(chat);
    const t: ChatTab = {
      id: "t" + Date.now(),
      title: chat.title,
      chatId,
      messages: msgs,
      composer: "",
      attachments: [],
    };
    setTabs((ts) => [...ts, t]);
    setActiveTab(t.id);
  };

  const openItemFromList = (id: string) => openChatSession(id);
  const openNoteFromList = (id: string) => {
    setOpenNote(id);
    setRightCollapsed(false);
  };

  const currentTab = tabs.find((t) => t.id === activeTab) || tabs[0]!;
  const isEmpty = currentTab.messages.length === 0;

  const updateTab = (patch: Partial<ChatTab>) => {
    setTabs((ts) => ts.map((t) => (t.id === activeTab ? { ...t, ...patch } : t)));
  };

  const handleSend = () => {
    const text = currentTab.composer.trim();
    if (!text) return;
    updateTab({
      messages: [...currentTab.messages, { role: "user", paragraphs: [text] }],
      composer: "",
      title: currentTab.messages.length === 0 ? truncate(text, 36) : currentTab.title,
    });
    setSending(true);
    setTimeout(() => {
      const modelName = MODELS.find((m) => m.id === model)?.name || "Claude";
      const reply = makeCannedAssistantReply(modelName);
      setTabs((ts) => ts.map((t) => (t.id === activeTab ? { ...t, messages: [...t.messages, reply] } : t)));
      setSending(false);
    }, 1400);
  };

  const composerProps: ComposerProps = {
    value: currentTab.composer,
    setValue: (v) => updateTab({ composer: v }),
    onSend: handleSend,
    attachments: currentTab.attachments,
    setAttachments: (v) => updateTab({ attachments: v }),
    model,
    setModel,
    mcpServers,
    setMcpServers,
    contextFolder,
    setContextFolder,
  };

  const commands: Command[] = [
    { label: "New chat session", icon: "plus", kbd: "⌘N", section: "Chat", run: newTab },
    { label: "Close tab", icon: "x", kbd: "⌘W", section: "Tabs", run: () => closeTab(activeTab) },
    { label: "Toggle left panel", icon: "sidebar-left", kbd: "⌘B", section: "View", run: () => setLeftCollapsed((c) => !c) },
    { label: "Toggle right panel", icon: "sidebar-right", kbd: "⌘⇧B", section: "View", run: () => setRightCollapsed((c) => !c) },
    { label: "Focus composer", icon: "pencil", kbd: "⌘/", section: "Chat" },
    { label: "Switch to Chat", icon: "message-square", kbd: "⌘1", section: "Navigation", run: () => setSection("chat") },
    { label: "Switch to Projects", icon: "folder-kanban", kbd: "⌘2", section: "Navigation", run: () => setSection("projects") },
    { label: "Switch to Memory", icon: "brain", kbd: "⌘3", section: "Navigation", run: () => setSection("memory") },
    { label: "Switch to Workflows", icon: "workflow", kbd: "⌘4", section: "Navigation", run: () => setSection("workflows") },
    { label: "Switch to Skills", icon: "sparkles", kbd: "⌘5", section: "Navigation", run: () => setSection("skills") },
    { label: "Compact context now", icon: "scroll", section: "Chat", run: () => addToast("Context compacted — 8.2k → 3.1k tokens") },
    { label: "Run workflow: Weekly status digest", icon: "play", section: "Workflows", run: () => addToast("Running Weekly status digest…") },
    { label: "Open settings", icon: "settings", kbd: "⌘,", section: "App", run: () => addToast("Settings: coming soon") },
    { label: "Report a bug", icon: "bug", section: "App" },
  ];

  return (
    <div className="app" data-screen-label={isEmpty ? "01 Empty state" : "02 Active chat"}>
      <div className="body-row">
        <Ribbon
          activeApp={activeApp}
          onActivate={setActiveApp}
          onPlaceholder={(name) => addToast(`${name} plugin not yet installed.`)}
          onOpenPlugins={() => addToast("Plugin marketplace coming soon.")}
          onOpenSettings={() => addToast("Settings: coming soon.")}
        />

        <div className={"pane pane-left " + (leftCollapsed ? "collapsed" : "")}>
          <LeftHeader
            collapsed={leftCollapsed}
            onToggle={() => setLeftCollapsed((c) => !c)}
            search={search}
            setSearch={setSearch}
            onNew={newTab}
          />
          <SectionSwitcher section={section} setSection={setSection} collapsed={leftCollapsed} />
          {!leftCollapsed && (
            <>
              {section === "chat" && (
                <ChatSection chats={CHATS} activeChatId={currentTab.chatId} onOpen={openItemFromList} search={search} />
              )}
              {section === "projects" && (
                <ProjectsSection
                  projects={PROJECTS}
                  chats={CHATS}
                  onOpen={openItemFromList}
                  onOpenNote={openNoteFromList}
                  activeChatId={currentTab.chatId}
                />
              )}
              {section === "memory" && <MemorySection onOpen={openNoteFromList} search={search} />}
              {section === "workflows" && <WorkflowsSection onRun={(n) => addToast(`Running ${n}…`)} />}
              {section === "skills" && <SkillsSection skills={skills} setSkills={setSkills} />}
            </>
          )}
          <LeftFooter collapsed={leftCollapsed} initials={USER.initials} fullName={USER.fullName} />
        </div>

        <div className="pane pane-main">
          <TabBar tabs={tabs} activeTab={activeTab} onActivate={setActiveTab} onClose={closeTab} onNew={newTab} />
          <div className="chat-pane">
            {isEmpty ? (
              <EmptyState composerProps={composerProps} name={USER.firstName} />
            ) : (
              <ChatView messages={currentTab.messages} thinking={sending} composerProps={composerProps} />
            )}
          </div>
        </div>

        <RightPanel
          collapsed={rightCollapsed}
          onToggle={() => setRightCollapsed((c) => !c)}
          activeChatId={currentTab.chatId}
          openNote={openNote}
          setOpenNote={setOpenNote}
        />
      </div>

      <StatusBar
        model={MODELS.find((m) => m.id === model)?.name || "—"}
        contextFolder="Memory"
        mcpActive={mcpServers.filter((s) => s.on).length}
        mcpTotal={mcpServers.length}
        sessionCount={CHATS.length}
        skillsOn={skills.filter((s) => s.on).length}
      />

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        commands={commands}
        onRun={(c) => c.run && c.run()}
      />

      <ToastStack toasts={toasts} />
    </div>
  );
}
