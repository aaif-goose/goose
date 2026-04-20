import { useState } from "react";
import { Icon } from "../Icon";
import { NOTES, SESSION_NOTES_BY_CHAT } from "../../data";
import type { NoteBlock } from "../../types";

type Scope = "session" | "project" | "all";

export function RightPanel({
  collapsed,
  onToggle,
  activeChatId,
  openNote,
  setOpenNote,
}: {
  collapsed: boolean;
  onToggle: () => void;
  activeChatId: string | null;
  openNote: string | null;
  setOpenNote: (id: string | null) => void;
}) {
  const [scope, setScope] = useState<Scope>("session");

  if (collapsed) {
    return (
      <div className="pane pane-right collapsed">
        <div className="right-header">
          <button className="icon-btn" onClick={onToggle} title="Expand panel">
            <Icon name="sidebar-right" size={14} />
          </button>
        </div>
        <div className="right-rail">
          <button className="icon-btn" onClick={onToggle} title="Notes">
            <Icon name="brain" size={15} />
          </button>
          <button className="icon-btn" title="Search">
            <Icon name="search" size={15} />
          </button>
        </div>
      </div>
    );
  }

  if (openNote) {
    const note = NOTES[openNote] || NOTES.n2!;
    return (
      <div className="pane pane-right">
        <div className="right-header">
          <button className="icon-btn" onClick={() => setOpenNote(null)} title="Back">
            <Icon name="chevron-left" size={14} />
          </button>
          <div className="title">{note.title}</div>
          <button className="icon-btn" title="More"><Icon name="more" size={14} /></button>
          <button className="icon-btn" onClick={onToggle} title="Collapse">
            <Icon name="x" size={13} />
          </button>
        </div>
        <div className="note-editor">
          <div className="note-editor-body">
            <div className="meta">
              <span>{note.kind === "wiki" ? "Wiki page" : "Note"}</span>
              <span>·</span>
              <span>Updated {note.updated}</span>
              <span>·</span>
              <span>{note.words} words</span>
            </div>
            {note.body.map((b: NoteBlock, i: number) => {
              if (b.type === "meta") return null;
              if (b.type === "h1") return <h1 key={i}>{b.text}</h1>;
              if (b.type === "h2") return <h2 key={i}>{b.text}</h2>;
              if (b.type === "p") return <p key={i}>{b.text}</p>;
              if (b.type === "ul") return <ul key={i}>{b.items.map((it, j) => <li key={j}>{it}</li>)}</ul>;
              return null;
            })}
          </div>
          <div className="note-footer">
            <span className="sync-dot" />
            <span>Saved</span>
            <span>·</span>
            <span>Last modified {note.updated}</span>
            <div style={{ flex: 1 }} />
            <span>{note.words} words</span>
          </div>
        </div>
      </div>
    );
  }

  const sessionNotes = (SESSION_NOTES_BY_CHAT[activeChatId ?? ""] || ["n2", "n3", "n4"])
    .map((id) => NOTES[id])
    .filter(Boolean);
  const projectNotes = Object.values(NOTES).slice(0, 6);
  const allNotes = Object.values(NOTES);
  const shown = scope === "session" ? sessionNotes : scope === "project" ? projectNotes : allNotes;

  return (
    <div className="pane pane-right">
      <div className="right-header">
        <div className="title">Notes for this session</div>
        <button className="icon-btn" title="New note"><Icon name="plus" size={14} /></button>
        <button className="icon-btn" onClick={onToggle} title="Collapse (⌘⇧B)">
          <Icon name="x" size={13} />
        </button>
      </div>
      <div className="scope-toggle">
        <button className={"scope-btn " + (scope === "session" ? "active" : "")} onClick={() => setScope("session")}>
          This session
        </button>
        <button className={"scope-btn " + (scope === "project" ? "active" : "")} onClick={() => setScope("project")}>
          This project
        </button>
        <button className={"scope-btn " + (scope === "all" ? "active" : "")} onClick={() => setScope("all")}>
          All memory
        </button>
      </div>
      <div className="notes-list">
        {shown.length === 0 ? (
          <div
            style={{
              padding: 24,
              textAlign: "center",
              color: "var(--color-text-muted)",
              fontSize: 13,
              lineHeight: 1.5,
            }}
          >
            No notes yet. I'll create notes automatically as we talk, or you can ask me to.
          </div>
        ) : (
          shown.map((n) => (
            <div key={n!.id} className="list-row" onClick={() => setOpenNote(n!.id)}>
              <Icon name={n!.kind === "wiki" ? "book" : "file-text"} size={13} className="row-icon" />
              <span className="row-label">{n!.title}</span>
              <span className="row-meta">{n!.updated}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
