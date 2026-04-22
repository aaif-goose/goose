// RightPanel.jsx — Memory notes list + full editor

const { useState: useStR } = React;

function RightPanel({ collapsed, onToggle, activeChatId, openNote, setOpenNote, variant }) {
  const [scope, setScope] = useStR('session');

  if (collapsed) {
    return (
      <div className="pane pane-right collapsed">
        <div className="right-header">
          <button className="icon-btn" onClick={onToggle} title="Expand panel"><Icon name="sidebar-right" size={14}/></button>
        </div>
        <div className="right-rail">
          <button className="icon-btn" onClick={onToggle} title="Notes"><Icon name="brain" size={15}/></button>
          <button className="icon-btn" title="Search"><Icon name="search" size={15}/></button>
        </div>
      </div>
    );
  }

  // Editor view
  if (variant === 'editor' || openNote) {
    const note = NOTES[openNote] || NOTES.n2;
    return (
      <div className="pane pane-right">
        <div className="right-header">
          <button className="icon-btn" onClick={() => setOpenNote(null)} title="Back"><Icon name="chevron-left" size={14}/></button>
          <div className="title">{note.title}</div>
          <button className="icon-btn" title="More"><Icon name="more" size={14}/></button>
          <button className="icon-btn" onClick={onToggle} title="Collapse"><Icon name="x" size={13}/></button>
        </div>
        <div className="note-editor">
          <div className="note-editor-body">
            <div className="meta">
              <span>{note.kind === 'wiki' ? 'Wiki page' : 'Note'}</span>
              <span>·</span>
              <span>Updated {note.updated}</span>
              <span>·</span>
              <span>{note.words} words</span>
            </div>
            {note.body.map((b, i) => {
              if (b.type === 'meta') return null;
              if (b.type === 'h1') return <h1 key={i}>{b.text}</h1>;
              if (b.type === 'h2') return <h2 key={i}>{b.text}</h2>;
              if (b.type === 'p') return <p key={i}>{b.text}</p>;
              if (b.type === 'ul') return <ul key={i}>{b.items.map((it, j) => <li key={j}>{it}</li>)}</ul>;
              return null;
            })}
          </div>
          <div className="note-footer">
            <span className="sync-dot"/>
            <span>Saved</span>
            <span>·</span>
            <span>Last modified {note.updated}</span>
            <div style={{flex: 1}}/>
            <span>{note.words} words</span>
          </div>
        </div>
      </div>
    );
  }

  // List view
  const sessionNotes = (SESSION_NOTES_BY_CHAT[activeChatId] || ['n2', 'n3', 'n4']).map(id => NOTES[id]).filter(Boolean);
  const projectNotes = Object.values(NOTES).slice(0, 6);
  const allNotes = Object.values(NOTES);
  const shown = scope === 'session' ? sessionNotes : scope === 'project' ? projectNotes : allNotes;

  return (
    <div className="pane pane-right">
      <div className="right-header">
        <div className="title">Notes for this session</div>
        <button className="icon-btn" title="New note"><Icon name="plus" size={14}/></button>
        <button className="icon-btn" onClick={onToggle} title="Collapse (⌘⇧B)"><Icon name="x" size={13}/></button>
      </div>
      <div className="scope-toggle">
        <button className={'scope-btn ' + (scope === 'session' ? 'active' : '')} onClick={() => setScope('session')}>This session</button>
        <button className={'scope-btn ' + (scope === 'project' ? 'active' : '')} onClick={() => setScope('project')}>This project</button>
        <button className={'scope-btn ' + (scope === 'all' ? 'active' : '')} onClick={() => setScope('all')}>All memory</button>
      </div>
      <div className="notes-list">
        {shown.length === 0 ? (
          <div style={{padding: 24, textAlign: 'center', color: 'var(--color-text-muted)', fontSize: 13, lineHeight: 1.5}}>
            No notes yet. I'll create notes automatically as we talk, or you can ask me to.
          </div>
        ) : shown.map(n => (
          <div key={n.id} className="list-row" onClick={() => setOpenNote(n.id)}>
            <Icon name={n.kind === 'wiki' ? 'book' : 'file-text'} size={13} className="row-icon"/>
            <span className="row-label">{n.title}</span>
            <span className="row-meta">{n.updated}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---- Command palette ----
function CommandPalette({ open, onClose, commands, onRun }) {
  const [q, setQ] = useStR('');
  const [sel, setSel] = useStR(0);
  if (!open) return null;
  const filtered = commands.filter(c => c.label.toLowerCase().includes(q.toLowerCase()));
  const onKey = e => {
    if (e.key === 'Escape') onClose();
    else if (e.key === 'ArrowDown') { e.preventDefault(); setSel(s => Math.min(s + 1, filtered.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel(s => Math.max(s - 1, 0)); }
    else if (e.key === 'Enter' && filtered[sel]) { onRun(filtered[sel]); onClose(); }
  };
  return (
    <div className="scrim" onClick={onClose}>
      <div className="palette" onClick={e => e.stopPropagation()}>
        <div className="palette-input-wrap">
          <span className="search-ico"><Icon name="search" size={15}/></span>
          <input className="palette-input" placeholder="Type a command or search…" autoFocus
                 value={q} onChange={e => { setQ(e.target.value); setSel(0); }} onKeyDown={onKey}/>
        </div>
        <div className="palette-results">
          {filtered.map((c, i) => (
            <button key={i} className={'palette-row ' + (i === sel ? 'active' : '')}
                    onClick={() => { onRun(c); onClose(); }} onMouseEnter={() => setSel(i)}>
              <Icon name={c.icon} size={14} className="ico"/>
              <span className="label">{c.label}</span>
              {c.section && <span className="section">{c.section}</span>}
              {c.kbd && <kbd style={{marginLeft: 8}}>{c.kbd}</kbd>}
            </button>
          ))}
          {filtered.length === 0 && (
            <div style={{padding: 24, textAlign: 'center', color: 'var(--color-text-muted)', fontSize: 13}}>
              No matching command
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---- Tweaks panel ----
function TweaksPanel({ open, onClose, tweaks, setTweaks }) {
  if (!open) return null;
  const update = (k, v) => setTweaks({...tweaks, [k]: v});
  const ACCENTS = [
    { id: 'violet', val: '#8b7cff' },
    { id: 'indigo', val: '#7aa2f7' },
    { id: 'teal', val: '#6fcf97' },
    { id: 'amber', val: '#e6b450' },
    { id: 'rose', val: '#e06c75' },
  ];
  return (
    <div className="tweaks-panel">
      <div className="tweaks-header">
        <Icon name="sliders" size={14}/>
        <span className="title">Tweaks</span>
        <button className="icon-btn tight" onClick={onClose}><Icon name="x" size={13}/></button>
      </div>
      <div className="tweaks-body">
        <div className="tweak-row">
          <label className="tweak-label">Accent color</label>
          <div className="color-swatches">
            {ACCENTS.map(a => (
              <div key={a.id} className={'swatch ' + (tweaks.accent === a.id ? 'active' : '')}
                   style={{background: a.val}}
                   onClick={() => update('accent', a.id)}/>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Empty state layout</label>
          <div className="tweak-options">
            {['centered', 'top', 'chips'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.emptyLayout === v ? 'active' : '')}
                      onClick={() => update('emptyLayout', v)}>{v}</button>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Section switcher style</label>
          <div className="tweak-options">
            {['icons-text', 'tabs', 'segmented'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.switcher === v ? 'active' : '')}
                      onClick={() => update('switcher', v)}>{v.replace('-', ' + ')}</button>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Assistant message style</label>
          <div className="tweak-options">
            {['column', 'bubbles'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.msgStyle === v ? 'active' : '')}
                      onClick={() => update('msgStyle', v)}>{v}</button>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Token popover variant</label>
          <div className="tweak-options">
            {['inline', 'floating'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.tokenPop === v ? 'active' : '')}
                      onClick={() => update('tokenPop', v)}>{v}</button>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Composer footer order</label>
          <div className="tweak-options">
            {['default', 'token-first', 'minimal'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.footerOrder === v ? 'active' : '')}
                      onClick={() => update('footerOrder', v)}>{v.replace('-', ' ')}</button>
            ))}
          </div>
        </div>
        <div className="tweak-row">
          <label className="tweak-label">Right panel</label>
          <div className="tweak-options">
            {['list', 'editor', 'collapsed'].map(v => (
              <button key={v} className={'tweak-chip ' + (tweaks.rightMode === v ? 'active' : '')}
                      onClick={() => update('rightMode', v)}>{v}</button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ---- Status bar ----
function StatusBar({ model, contextFolder, mcpActive, sessionCount, skillsOn }) {
  return (
    <div className="status-bar">
      <div className="item"><span className="sync-dot"/><span className="status-ok">Synced</span></div>
      <div className="item">Folder: {contextFolder}</div>
      <div className="item">MCP: {mcpActive}/7</div>
      <div className="item">Skills: {skillsOn} on</div>
      <div className="spacer"/>
      <div className="item">{model}</div>
      <div className="item">{sessionCount} sessions</div>
      <div className="item">⌘K for commands</div>
    </div>
  );
}

Object.assign(window, { RightPanel, CommandPalette, TweaksPanel, StatusBar });
