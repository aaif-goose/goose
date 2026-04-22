import { useEffect, useRef, useState } from "react";
import type { SessionNotification } from "@agentclientprotocol/sdk";
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
import { SettingsModal } from "./components/settings/SettingsModal";
import { DiagnosticsModal } from "./components/diagnostics/DiagnosticsModal";
import { CHATS, MCP_SERVERS, MODELS, SECTIONS, SKILLS, USER } from "./data";
import type { ChatTab, Command, McpServer, Message, SectionId, Skill, Toast, ToolEvent } from "./types";
import { generateFakeConversation, truncate } from "./mockReply";
import { initAcp, sendMessage, startSession } from "./services/acp";
import {
  listMemoryNotes,
  listProjectNotes,
  listProjects,
  type FolderNote,
  type FolderProject,
} from "./services/folders";
import { listRecipes, loadRecipePrompt, type Recipe } from "./services/recipes";
import { getSettings, updateSettings, type Settings } from "./services/settings";
import { loadUiState, saveUiState } from "./services/persistence";

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
  const [hydrated, setHydrated] = useState(false);

  // Phase 3 state: settings + folder-backed data + recipes.
  const [settings, setSettings] = useState<Settings>({});
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [memoryNotes, setMemoryNotes] = useState<FolderNote[]>([]);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [projects, setProjects] = useState<FolderProject[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(false);
  const [projectNotes, setProjectNotes] = useState<Record<string, FolderNote[]>>({});
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [recipes, setRecipes] = useState<Recipe[]>([]);
  const [recipesLoading, setRecipesLoading] = useState(false);

  // Maps goose sessionId → local tab id so streaming notifications route correctly.
  const sessionToTab = useRef<Map<string, string>>(new Map());
  // Most recently started assistant message per goose session (for chunk coalescing).
  const streamingMsgId = useRef<Map<string, string>>(new Map());

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

  // ---------- Persistence: hydrate on mount ----------
  useEffect(() => {
    let cancelled = false;
    loadUiState()
      .then((persisted) => {
        if (cancelled || !persisted) return;
        // Clean: clear gooseSessionId (ACP sessions don't survive restart)
        // and drop streaming flags.
        const restoredTabs: ChatTab[] = persisted.tabs.map((t) => ({
          ...t,
          gooseSessionId: undefined,
          messages: t.messages.map((m) =>
            m.streaming ? { ...m, streaming: false } : m,
          ),
        }));
        if (restoredTabs.length > 0) {
          setTabs(restoredTabs);
          const wantedActive =
            restoredTabs.find((t) => t.id === persisted.activeTab)?.id ??
            restoredTabs[0]!.id;
          setActiveTab(wantedActive);
        }
        setSection(persisted.section);
        setLeftCollapsed(persisted.leftCollapsed);
        setRightCollapsed(persisted.rightCollapsed);
        setOpenNote(persisted.openNote);
      })
      .catch((err) => console.warn("[persistence] hydrate failed", err))
      .finally(() => {
        if (!cancelled) setHydrated(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // ---------- Persistence: debounced save after hydrate ----------
  useEffect(() => {
    if (!hydrated) return;
    const handle = window.setTimeout(() => {
      void saveUiState({
        tabs,
        activeTab,
        section,
        leftCollapsed,
        rightCollapsed,
        openNote,
      });
    }, 500);
    return () => window.clearTimeout(handle);
  }, [hydrated, tabs, activeTab, section, leftCollapsed, rightCollapsed, openNote]);

  // ---------- Settings, folders, recipes ----------
  useEffect(() => {
    getSettings()
      .then((s) => setSettings(s))
      .catch((err) => console.error("[settings] load failed", err));
    listRecipes()
      .then((r) => setRecipes(r))
      .catch((err) => console.error("[recipes] load failed", err))
      .finally(() => setRecipesLoading(false));
    setRecipesLoading(true);
  }, []);

  useEffect(() => {
    const dir = settings.memoryDir;
    if (!dir) {
      setMemoryNotes([]);
      return;
    }
    let cancelled = false;
    setMemoryLoading(true);
    listMemoryNotes(dir)
      .then((notes) => {
        if (!cancelled) setMemoryNotes(notes);
      })
      .catch((err) => {
        if (!cancelled) {
          console.error("[memory] load failed", err);
          addToast(`Memory load failed: ${String(err)}`);
        }
      })
      .finally(() => {
        if (!cancelled) setMemoryLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.memoryDir]);

  useEffect(() => {
    const dir = settings.projectsDir;
    if (!dir) {
      setProjects([]);
      setProjectNotes({});
      return;
    }
    let cancelled = false;
    setProjectsLoading(true);
    listProjects(dir)
      .then((ps) => {
        if (!cancelled) setProjects(ps);
      })
      .catch((err) => {
        if (!cancelled) {
          console.error("[projects] load failed", err);
          addToast(`Projects load failed: ${String(err)}`);
        }
      })
      .finally(() => {
        if (!cancelled) setProjectsLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.projectsDir]);

  const onOpenProject = async (p: FolderProject) => {
    setActiveProjectId(p.id);
    if (projectNotes[p.id]) return;
    try {
      const notes = await listProjectNotes(p.path);
      setProjectNotes((prev) => ({ ...prev, [p.id]: notes }));
    } catch (err) {
      addToast(`Open project failed: ${String(err)}`);
    }
  };

  const saveSettings = async (next: Settings) => {
    const saved = await updateSettings(next);
    setSettings(saved);
  };

  // ---------- ACP notification handler ----------
  const handleNotification = (n: SessionNotification) => {
    const gooseSessionId = n.sessionId;
    const tabId = sessionToTab.current.get(gooseSessionId);
    if (!tabId) return;
    const update = n.update;

    const mutateTab = (fn: (msgs: Message[]) => Message[]) => {
      setTabs((ts) => ts.map((t) => (t.id === tabId ? { ...t, messages: fn(t.messages) } : t)));
    };

    switch (update.sessionUpdate) {
      case "agent_message_chunk": {
        const content = update.content;
        if (content.type !== "text" || typeof content.text !== "string") break;
        const chunkText = content.text;
        const incomingMsgId =
          (update as { messageId?: string }).messageId ?? streamingMsgId.current.get(gooseSessionId);
        mutateTab((msgs) => {
          const last = msgs[msgs.length - 1];
          if (last && last.role === "assistant" && last.streaming && (!incomingMsgId || last.id === incomingMsgId)) {
            const paragraphs = [...(last.paragraphs ?? [""])];
            paragraphs[0] = (paragraphs[0] ?? "") + chunkText;
            return [...msgs.slice(0, -1), { ...last, paragraphs }];
          }
          const id = incomingMsgId ?? crypto.randomUUID();
          streamingMsgId.current.set(gooseSessionId, id);
          return [
            ...msgs,
            { id, role: "assistant", streaming: true, paragraphs: [chunkText], tools: [] },
          ];
        });
        if (incomingMsgId) streamingMsgId.current.set(gooseSessionId, incomingMsgId);
        break;
      }
      case "tool_call": {
        const toolEvent: ToolEvent = {
          id: update.toolCallId,
          name: update.title,
          summary: "Running\u2026",
          status: "executing",
        };
        mutateTab((msgs) => {
          const last = msgs[msgs.length - 1];
          if (last && last.role === "assistant" && last.streaming) {
            return [...msgs.slice(0, -1), { ...last, tools: [...(last.tools ?? []), toolEvent] }];
          }
          const id = crypto.randomUUID();
          streamingMsgId.current.set(gooseSessionId, id);
          return [
            ...msgs,
            { id, role: "assistant", streaming: true, paragraphs: [""], tools: [toolEvent] },
          ];
        });
        break;
      }
      case "tool_call_update": {
        const status: ToolEvent["status"] =
          update.status === "completed"
            ? "completed"
            : update.status === "failed"
              ? "failed"
              : "executing";
        mutateTab((msgs) =>
          msgs.map((m) => {
            if (!m.tools?.some((t) => t.id === update.toolCallId)) return m;
            return {
              ...m,
              tools: m.tools.map((t) =>
                t.id === update.toolCallId
                  ? {
                      ...t,
                      status,
                      name: update.title ?? t.name,
                      summary:
                        status === "completed"
                          ? "Done"
                          : status === "failed"
                            ? "Failed"
                            : t.summary,
                    }
                  : t,
              ),
            };
          }),
        );
        break;
      }
      default:
        break;
    }
  };

  // Warm the ACP client + register our handler once.
  useEffect(() => {
    let cancelled = false;
    initAcp((n) => {
      if (!cancelled) handleNotification(n);
    }).catch((err: unknown) => {
      console.error("[acp] init failed", err);
      addToast(`ACP init failed: ${String(err)}`);
    });
    return () => {
      cancelled = true;
    };
    // handleNotification closes over setTabs which is stable; addToast is defined above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
  const openNoteFromList = (path: string) => {
    setOpenNote(path);
    setRightCollapsed(false);
  };

  // Run a Goose recipe: new tab, new session, send the recipe's prompt.
  const runRecipe = async (recipe: Recipe) => {
    const tabId = "t" + Date.now();
    const title = recipe.title || recipe.name;
    setTabs((ts) => [
      ...ts,
      {
        id: tabId,
        title,
        chatId: null,
        messages: [],
        composer: "",
        attachments: [],
      },
    ]);
    setActiveTab(tabId);
    try {
      const prompt = await loadRecipePrompt(recipe.path);
      const gooseSessionId = await startSession();
      sessionToTab.current.set(gooseSessionId, tabId);
      setTabs((ts) =>
        ts.map((t) =>
          t.id === tabId
            ? {
                ...t,
                gooseSessionId,
                messages: [{ role: "user", paragraphs: [prompt] }],
              }
            : t,
        ),
      );
      setSending(true);
      await sendMessage(gooseSessionId, prompt);
    } catch (err) {
      console.error("[recipe] run failed", err);
      addToast(`Recipe failed: ${String(err)}`);
    } finally {
      setSending(false);
      setTabs((ts) =>
        ts.map((t) =>
          t.id !== tabId
            ? t
            : {
                ...t,
                messages: t.messages.map((m, i, arr) =>
                  i === arr.length - 1 && m.role === "assistant" && m.streaming
                    ? { ...m, streaming: false }
                    : m,
                ),
              },
        ),
      );
    }
  };

  const currentTab = tabs.find((t) => t.id === activeTab) || tabs[0]!;
  const isEmpty = currentTab.messages.length === 0;

  const updateTab = (patch: Partial<ChatTab>) => {
    setTabs((ts) => ts.map((t) => (t.id === activeTab ? { ...t, ...patch } : t)));
  };

  const handleSend = async () => {
    const tab = currentTab;
    const text = tab.composer.trim();
    if (!text) return;

    const userMsg: Message = { role: "user", paragraphs: [text] };
    const nextTitle = tab.messages.length === 0 ? truncate(text, 36) : tab.title;

    // Push user message immediately; clear composer.
    setTabs((ts) =>
      ts.map((t) =>
        t.id === tab.id
          ? { ...t, messages: [...t.messages, userMsg], composer: "", title: nextTitle }
          : t,
      ),
    );
    setSending(true);

    try {
      let gooseSessionId = tab.gooseSessionId;
      if (!gooseSessionId) {
        gooseSessionId = await startSession();
        sessionToTab.current.set(gooseSessionId, tab.id);
        setTabs((ts) =>
          ts.map((t) => (t.id === tab.id ? { ...t, gooseSessionId } : t)),
        );
      }
      await sendMessage(gooseSessionId, text);
    } catch (err) {
      console.error("[acp] sendMessage failed", err);
      addToast(`Send failed: ${String(err)}`);
    } finally {
      // Mark the streaming assistant message as complete.
      setTabs((ts) =>
        ts.map((t) =>
          t.id !== tab.id
            ? t
            : {
                ...t,
                messages: t.messages.map((m, i, arr) =>
                  i === arr.length - 1 && m.role === "assistant" && m.streaming
                    ? { ...m, streaming: false }
                    : m,
                ),
              },
        ),
      );
      setSending(false);
    }
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
    onReportBug: () => setDiagnosticsOpen(true),
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
    { label: "Open settings", icon: "settings", kbd: "⌘,", section: "App", run: () => setSettingsOpen(true) },
    { label: "Report a bug", icon: "bug", section: "App", run: () => setDiagnosticsOpen(true) },
  ];

  return (
    <div className="app" data-screen-label={isEmpty ? "01 Empty state" : "02 Active chat"}>
      <div className="body-row">
        <Ribbon
          activeApp={activeApp}
          onActivate={setActiveApp}
          onPlaceholder={(name) => addToast(`${name} plugin not yet installed.`)}
          onOpenPlugins={() => addToast("Plugin marketplace coming soon.")}
          onOpenSettings={() => setSettingsOpen(true)}
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
                  projects={projects}
                  activeProjectId={activeProjectId}
                  projectNotes={projectNotes}
                  onOpenProject={onOpenProject}
                  onOpenNote={openNoteFromList}
                  onOpenSettings={() => setSettingsOpen(true)}
                  loading={projectsLoading}
                  configured={!!settings.projectsDir}
                />
              )}
              {section === "memory" && (
                <MemorySection
                  notes={memoryNotes}
                  onOpen={openNoteFromList}
                  search={search}
                  onOpenSettings={() => setSettingsOpen(true)}
                  loading={memoryLoading}
                  configured={!!settings.memoryDir}
                />
              )}
              {section === "workflows" && (
                <WorkflowsSection recipes={recipes} onRun={runRecipe} loading={recipesLoading} />
              )}
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
          folderNotes={memoryNotes}
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

      <SettingsModal
        open={settingsOpen}
        initial={settings}
        onClose={() => setSettingsOpen(false)}
        onSave={saveSettings}
      />

      <DiagnosticsModal
        open={diagnosticsOpen}
        onClose={() => setDiagnosticsOpen(false)}
        currentTab={currentTab}
        model={model}
        mcpServers={mcpServers}
        memoryDir={settings.memoryDir}
        onToast={addToast}
      />

      <ToastStack toasts={toasts} />
    </div>
  );
}
