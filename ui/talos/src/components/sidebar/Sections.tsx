import { useState } from "react";
import { Icon } from "../Icon";
import { SECTIONS, NOTES, WORKFLOWS } from "../../data";
import type { Chat, Project, SectionId, Skill } from "../../types";

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

// ---------- Projects section ----------
export function ProjectsSection({
  projects,
  chats,
  onOpen,
  onOpenNote,
  activeChatId,
}: {
  projects: Project[];
  chats: Chat[];
  onOpen: (id: string) => void;
  onOpenNote: (id: string) => void;
  activeChatId: string | null;
}) {
  const [open, setOpen] = useState<Record<string, boolean>>(() => ({
    [projects[0]!.id]: true,
    [projects[1]!.id]: true,
  }));
  const toggle = (id: string) => setOpen((o) => ({ ...o, [id]: !o[id] }));
  return (
    <div className="section-body">
      <div className="list-group">
        <div className="list-heading"><span>Projects</span><span className="count">{projects.length}</span></div>
        {projects.map((p) => {
          const projChats = chats.filter((c) => c.project === p.id);
          const projNotes = (p.notes || []).map((nid) => NOTES[nid]).filter(Boolean);
          const o = !!open[p.id];
          return (
            <div key={p.id}>
              <div className={"list-row " + (o ? "open" : "")} onClick={() => toggle(p.id)}>
                <Icon name="chevron-right" size={12} className="chev" />
                <Icon name="folder" size={13} className="row-icon" />
                <span className="row-label">{p.name}</span>
                <span className="row-meta">{projChats.length + projNotes.length}</span>
              </div>
              {o && (
                <>
                  {projChats.map((c) => (
                    <div
                      key={c.id}
                      className={"list-row nested " + (activeChatId === c.id ? "active" : "")}
                      onClick={() => onOpen(c.id)}
                    >
                      <Icon name="message-square" size={12} className="row-icon" />
                      <span className="row-label">{c.title}</span>
                    </div>
                  ))}
                  {projNotes.map((n) => (
                    <div key={n!.id} className="list-row nested" onClick={() => onOpenNote(n!.id)}>
                      <Icon name="file-text" size={12} className="row-icon" />
                      <span className="row-label">{n!.title}</span>
                    </div>
                  ))}
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------- Memory section ----------
export function MemorySection({ onOpen, search }: { onOpen: (id: string) => void; search: string }) {
  const q = search.toLowerCase();
  const all = Object.values(NOTES).filter((n) => !q || n.title.toLowerCase().includes(q));
  const wiki = all.filter((n) => n.kind === "wiki");
  const notes = all.filter((n) => n.kind === "note");
  return (
    <div className="section-body">
      {wiki.length > 0 && (
        <div className="list-group">
          <div className="list-heading"><span>Wiki</span><span className="count">{wiki.length}</span></div>
          {wiki.map((n) => (
            <div key={n.id} className="list-row" onClick={() => onOpen(n.id)}>
              <Icon name="book" size={13} className="row-icon" />
              <span className="row-label">{n.title}</span>
            </div>
          ))}
        </div>
      )}
      <div className="list-group">
        <div className="list-heading"><span>Recent notes</span><span className="count">{notes.length}</span></div>
        {notes.map((n) => (
          <div key={n.id} className="list-row" onClick={() => onOpen(n.id)}>
            <Icon name="file-text" size={13} className="row-icon" />
            <span className="row-label">{n.title}</span>
            <span className="row-meta">{n.updated}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------- Workflows section ----------
export function WorkflowsSection({ onRun }: { onRun: (name: string) => void }) {
  return (
    <div className="section-body">
      <div className="list-heading"><span>Workflows</span><span className="count">{WORKFLOWS.length}</span></div>
      {WORKFLOWS.map((w) => (
        <div key={w.id} className="workflow-row">
          <div className="wf-info">
            <div className="wf-name">{w.name}</div>
            <div className="wf-meta">{w.steps} steps · last run {w.lastRun}</div>
          </div>
          <button className="run-btn" onClick={() => onRun(w.name)}>
            <Icon name="play" size={10} />
          </button>
          <button className="icon-btn tight"><Icon name="more" size={13} /></button>
        </div>
      ))}
    </div>
  );
}

// ---------- Skills section ----------
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
