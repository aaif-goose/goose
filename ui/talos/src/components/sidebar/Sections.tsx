import { useState } from "react";
import { Icon } from "../Icon";
import { SECTIONS } from "../../data";
import type { Chat, Project, SectionId, Skill } from "../../types";
import { formatRelativeTime, type FolderNote, type FolderProject } from "../../services/folders";
import type { Recipe } from "../../services/recipes";

export function SectionSwitcher({
  section,
  setSection,
  collapsed,
}: {
  section: SectionId;
  setSection: (id: SectionId) => void;
  collapsed: boolean;
}) {
  return (
    <div className="section-switcher style-icons-text">
      {SECTIONS.map((s) => {
        const active = s.id === section;
        return (
          <button
            key={s.id}
            className={"section-btn " + (active ? "active" : "")}
            onClick={() => setSection(s.id)}
            title={s.label}
          >
            <Icon name={s.icon as never} size={15} />
            {!collapsed && (
              <span className={"label " + (active ? "" : "hidden")}>{s.label}</span>
            )}
          </button>
        );
      })}
    </div>
  );
}

// ---------- Chat section ----------
const CHAT_GROUPS: Array<{ id: Chat["group"]; label: string }> = [
  { id: "today", label: "Today" },
  { id: "yesterday", label: "Yesterday" },
  { id: "this-week", label: "This week" },
  { id: "older", label: "Older" },
];

export function ChatSection({
  chats,
  activeChatId,
  onOpen,
  search,
}: {
  chats: Chat[];
  activeChatId: string | null;
  onOpen: (id: string) => void;
  search: string;
}) {
  const q = search.toLowerCase();
  const filter = (c: Chat) => !q || c.title.toLowerCase().includes(q);
  const pinned = chats.filter((c) => c.pinned).filter(filter);
  const recents = chats.filter((c) => !c.pinned).filter(filter);
  return (
    <div className="section-body">
      {pinned.length > 0 && (
        <div className="list-group">
          <div className="list-heading"><span>Pinned</span><span className="count">{pinned.length}</span></div>
          {pinned.map((c) => (
            <div
              key={c.id}
              className={"list-row " + (activeChatId === c.id ? "active" : "")}
              onClick={() => onOpen(c.id)}
            >
              <Icon name="pin" size={12} className="row-icon" />
              <span className="row-label">{c.title}</span>
              <span className="row-meta">{c.when}</span>
            </div>
          ))}
        </div>
      )}
      {CHAT_GROUPS.map((g) => {
        const items = recents.filter((c) => c.group === g.id);
        if (items.length === 0) return null;
        return (
          <div key={g.id} className="list-group">
            <div className="list-heading"><span>{g.label}</span><span className="count">{items.length}</span></div>
            {items.map((c) => (
              <div
                key={c.id}
                className={"list-row " + (activeChatId === c.id ? "active" : "")}
                onClick={() => onOpen(c.id)}
              >
                <Icon name="message-square" size={13} className="row-icon" />
                <span className="row-label">{c.title}</span>
                <span className="row-meta">{c.when}</span>
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

// ---------- Empty state helper (used by folder-backed sections) ----------
function SectionEmpty({ title, hint, onOpenSettings }: { title: string; hint: string; onOpenSettings: () => void }) {
  return (
    <div className="section-body">
      <div
        style={{
          padding: 24,
          display: "flex",
          flexDirection: "column",
          gap: 8,
          color: "var(--color-text-muted)",
          fontSize: "var(--text-sm)",
          lineHeight: 1.5,
        }}
      >
        <div style={{ color: "var(--color-text-secondary)", fontWeight: 600 }}>{title}</div>
        <div>{hint}</div>
        <button className="btn ghost" style={{ alignSelf: "flex-start" }} onClick={onOpenSettings}>
          <Icon name="settings" size={12} /> Open Settings
        </button>
      </div>
    </div>
  );
}

// ---------- Projects section (folder-backed) ----------
export function ProjectsSection({
  projects,
  activeProjectId,
  projectNotes,
  onOpenProject,
  onOpenNote,
  onOpenSettings,
  loading,
  configured,
}: {
  projects: FolderProject[];
  activeProjectId: string | null;
  projectNotes: Record<string, FolderNote[]>;
  onOpenProject: (p: FolderProject) => void;
  onOpenNote: (path: string) => void;
  onOpenSettings: () => void;
  loading: boolean;
  configured: boolean;
}) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const toggle = (id: string) => setOpen((o) => ({ ...o, [id]: !o[id] }));

  if (!configured) {
    return (
      <SectionEmpty
        title="No projects folder yet"
        hint="Point Talos at a folder whose subfolders are your projects."
        onOpenSettings={onOpenSettings}
      />
    );
  }

  return (
    <div className="section-body">
      <div className="list-group">
        <div className="list-heading">
          <span>Projects</span>
          <span className="count">{loading ? "\u2026" : projects.length}</span>
        </div>
        {projects.length === 0 && !loading && (
          <div style={{ padding: "8px 16px", color: "var(--color-text-muted)", fontSize: "var(--text-sm)" }}>
            Nothing here yet.
          </div>
        )}
        {projects.map((p) => {
          const isOpen = !!open[p.id];
          const notes = projectNotes[p.id] ?? [];
          return (
            <div key={p.id}>
              <div
                className={"list-row " + (isOpen ? "open" : "") + (activeProjectId === p.id ? " active" : "")}
                onClick={() => {
                  toggle(p.id);
                  onOpenProject(p);
                }}
              >
                <Icon name="chevron-right" size={12} className="chev" />
                <Icon name="folder" size={13} className="row-icon" />
                <span className="row-label">{p.name}</span>
                <span className="row-meta">{p.noteCount}</span>
              </div>
              {isOpen && notes.map((n) => (
                <div key={n.id} className="list-row nested" onClick={() => onOpenNote(n.path)}>
                  <Icon name={n.kind === "wiki" ? "book" : "file-text"} size={12} className="row-icon" />
                  <span className="row-label">{n.title}</span>
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------- Memory section (folder-backed) ----------
export function MemorySection({
  notes,
  onOpen,
  search,
  onOpenSettings,
  loading,
  configured,
}: {
  notes: FolderNote[];
  onOpen: (path: string) => void;
  search: string;
  onOpenSettings: () => void;
  loading: boolean;
  configured: boolean;
}) {
  if (!configured) {
    return (
      <SectionEmpty
        title="No memory folder yet"
        hint="Pick a folder of markdown / text files to use as your persistent memory."
        onOpenSettings={onOpenSettings}
      />
    );
  }
  const q = search.toLowerCase();
  const filtered = notes.filter((n) => !q || n.title.toLowerCase().includes(q));
  const wiki = filtered.filter((n) => n.kind === "wiki");
  const rest = filtered.filter((n) => n.kind !== "wiki");
  return (
    <div className="section-body">
      {wiki.length > 0 && (
        <div className="list-group">
          <div className="list-heading"><span>Wiki</span><span className="count">{wiki.length}</span></div>
          {wiki.map((n) => (
            <div key={n.id} className="list-row" onClick={() => onOpen(n.path)}>
              <Icon name="book" size={13} className="row-icon" />
              <span className="row-label">{n.title}</span>
            </div>
          ))}
        </div>
      )}
      <div className="list-group">
        <div className="list-heading">
          <span>Notes</span>
          <span className="count">{loading ? "\u2026" : rest.length}</span>
        </div>
        {rest.length === 0 && !loading && (
          <div style={{ padding: "8px 16px", color: "var(--color-text-muted)", fontSize: "var(--text-sm)" }}>
            Nothing matches.
          </div>
        )}
        {rest.map((n) => (
          <div key={n.id} className="list-row" onClick={() => onOpen(n.path)}>
            <Icon name="file-text" size={13} className="row-icon" />
            <span className="row-label">{n.title}</span>
            <span className="row-meta">{formatRelativeTime(n.updatedMs)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------- Workflows (Goose recipes) ----------
export function WorkflowsSection({
  recipes,
  onRun,
  loading,
}: {
  recipes: Recipe[];
  onRun: (recipe: Recipe) => void;
  loading: boolean;
}) {
  return (
    <div className="section-body">
      <div className="list-heading">
        <span>Recipes</span>
        <span className="count">{loading ? "\u2026" : recipes.length}</span>
      </div>
      {recipes.length === 0 && !loading && (
        <div style={{ padding: "12px 16px", color: "var(--color-text-muted)", fontSize: "var(--text-sm)", lineHeight: 1.5 }}>
          No recipes found in <code>~/.config/goose/recipes</code>, <code>.goose/recipes</code>, or <code>$GOOSE_RECIPE_PATH</code>.
        </div>
      )}
      {recipes.map((r) => (
        <div key={r.id} className="workflow-row">
          <div className="wf-info">
            <div className="wf-name">{r.title || r.name}</div>
            <div className="wf-meta" title={r.path}>
              {r.description || r.path}
            </div>
          </div>
          <button className="run-btn" title="Run recipe" onClick={() => onRun(r)}>
            <Icon name="play" size={10} />
          </button>
        </div>
      ))}
    </div>
  );
}

// ---------- Skills section (UI-only stub) ----------
export function SkillsSection({
  skills,
  setSkills,
}: {
  skills: Skill[];
  setSkills: (next: Skill[]) => void;
}) {
  const toggle = (id: string) => setSkills(skills.map((s) => (s.id === id ? { ...s, on: !s.on } : s)));
  const activeCount = skills.filter((s) => s.on).length;
  return (
    <div className="section-body">
      <div className="list-heading">
        <span>Skills</span>
        <span className="count">{activeCount} / {skills.length} on</span>
      </div>
      {skills.map((s) => (
        <div key={s.id} className="skill-row">
          <div className="skill-info">
            <div className="skill-name">{s.name}</div>
            <div className="skill-desc">{s.desc}</div>
          </div>
          <button className="icon-btn tight" title="Skill details">
            <Icon name="info" size={13} />
          </button>
          <button
            className={"toggle " + (s.on ? "on" : "")}
            onClick={() => toggle(s.id)}
            aria-label={s.on ? "Disable skill" : "Enable skill"}
          />
        </div>
      ))}
    </div>
  );
}

// Keep Project type imported for parity (legacy static projects removed).
export type { Project };
