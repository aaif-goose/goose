import { useEffect, useState } from "react";
import { Icon } from "../Icon";
import { NOTES, SESSION_NOTES_BY_CHAT } from "../../data";
import type { NoteBlock } from "../../types";
import { formatRelativeTime, readNote, type FolderNote } from "../../services/folders";

type Scope = "session" | "project" | "all";

export function RightPanel({
  collapsed,
  onToggle,
  activeChatId,
  openNote,
  setOpenNote,
  folderNotes,
}: {
  collapsed: boolean;
  onToggle: () => void;
  activeChatId: string | null;
  openNote: string | null;
  setOpenNote: (id: string | null) => void;
  /** Notes sourced from the configured Memory folder; used for the "All memory" scope and session fallbacks. */
  folderNotes: FolderNote[];
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
    return (
      <NoteView path={openNote} onBack={() => setOpenNote(null)} onClose={onToggle} />
    );
  }

  // Sample session notes (from static sample data) plus folder notes.
  const sampleSessionNotes = (SESSION_NOTES_BY_CHAT[activeChatId ?? ""] || [])
    .map((id) => NOTES[id])
    .filter(Boolean) as Array<{ id: string; title: string; kind: string; updated: string; path?: undefined }>;

  const shown = (() => {
    if (scope === "all") return folderNotes;
    if (scope === "session") {
      return sampleSessionNotes.map((n) => ({
        id: n.id,
        title: n.title,
        kind: n.kind,
        updated: n.updated,
        path: n.id, // static ids are handled by NoteView
      }));
    }
    return folderNotes;
  })();

  return (
    <div className="pane pane-right">
      <div className="right-header">
        <div className="title">Notes for this session</div>
        <button className="icon-btn" title="New note"><Icon name="plus" size={14} /></button>
        <button className="icon-btn" onClick={onToggle} title="Collapse (\u2318\u21e7B)">
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
            No notes yet.
          </div>
        ) : (
          shown.map((n) => {
            const meta =
              "updated" in n && n.updated
                ? n.updated
                : "updatedMs" in n
                  ? formatRelativeTime((n as unknown as FolderNote).updatedMs)
                  : "";
            return (
              <div key={n.id} className="list-row" onClick={() => setOpenNote((n as { path: string }).path)}>
                <Icon name={n.kind === "wiki" ? "book" : "file-text"} size={13} className="row-icon" />
                <span className="row-label">{n.title}</span>
                <span className="row-meta">{meta}</span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

// ---------- Note view: static sample OR folder-backed file ----------

function renderSampleBody(body: NoteBlock[]) {
  return body.map((b, i) => {
    if (b.type === "meta") return null;
    if (b.type === "h1") return <h1 key={i}>{b.text}</h1>;
    if (b.type === "h2") return <h2 key={i}>{b.text}</h2>;
    if (b.type === "p") return <p key={i}>{b.text}</p>;
    if (b.type === "ul") return <ul key={i}>{b.items.map((it, j) => <li key={j}>{it}</li>)}</ul>;
    return null;
  });
}

function NoteView({ path, onBack, onClose }: { path: string; onBack: () => void; onClose: () => void }) {
  const sample = NOTES[path];

  const [content, setContent] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (sample) return; // no fetch needed
    let cancelled = false;
    setContent(null);
    setError(null);
    readNote(path)
      .then((text) => {
        if (!cancelled) setContent(text);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [path, sample]);

  if (sample) {
    return (
      <div className="pane pane-right">
        <div className="right-header">
          <button className="icon-btn" onClick={onBack} title="Back">
            <Icon name="chevron-left" size={14} />
          </button>
          <div className="title">{sample.title}</div>
          <button className="icon-btn" onClick={onClose} title="Collapse">
            <Icon name="x" size={13} />
          </button>
        </div>
        <div className="note-editor">
          <div className="note-editor-body">
            <div className="meta">
              <span>{sample.kind === "wiki" ? "Wiki page" : "Note"}</span>
              <span>\u00b7</span>
              <span>Updated {sample.updated}</span>
              <span>\u00b7</span>
              <span>{sample.words} words</span>
            </div>
            {renderSampleBody(sample.body)}
          </div>
        </div>
      </div>
    );
  }

  const filename = path.split("/").pop() || path;
  return (
    <div className="pane pane-right">
      <div className="right-header">
        <button className="icon-btn" onClick={onBack} title="Back">
          <Icon name="chevron-left" size={14} />
        </button>
        <div className="title" title={path}>{filename}</div>
        <button className="icon-btn" onClick={onClose} title="Collapse">
          <Icon name="x" size={13} />
        </button>
      </div>
      <div className="note-editor">
        <div className="note-editor-body">
          <div className="meta" style={{ fontFamily: "var(--font-mono)", wordBreak: "break-all" }}>
            {path}
          </div>
          {error ? (
            <p style={{ color: "var(--color-danger)" }}>Failed to read file: {error}</p>
          ) : content === null ? (
            <p style={{ color: "var(--color-text-muted)" }}>Loading\u2026</p>
          ) : (
            <pre
              style={{
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
                fontFamily: "var(--font-reading)",
                fontSize: "var(--text-md)",
                lineHeight: "var(--lh-reading)",
                background: "transparent",
                border: 0,
                padding: 0,
              }}
            >
              {content}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}
