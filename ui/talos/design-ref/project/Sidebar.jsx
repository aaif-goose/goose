// Sidebar.jsx — Ribbon + Left Panel sections

const { useState: useSt1 } = React;

const RIBBON_APPS = [
  { id: 'chat', emoji: '💬', label: 'Chat', live: true },
  { id: 'email', emoji: '✉️', label: 'Email', live: false },
  { id: 'calendar', emoji: '📅', label: 'Calendar', live: false },
  { id: 'projects', emoji: '📋', label: 'Projects', live: false },
  { id: 'editor', code: true, label: 'XML / JSON / Groovy', live: false },
];

function Ribbon({ activeApp, onActivate, onPlaceholder, onOpenPlugins, onOpenSettings }) {
  return (
    <div className="ribbon">
      <div className="ribbon-group">
        {RIBBON_APPS.map(app => (
          <button
            key={app.id}
            className={'ribbon-btn ' + (activeApp === app.id ? 'active' : '')}
            onClick={() => app.live ? onActivate(app.id) : onPlaceholder(app.label)}
            title={app.label}
          >
            {app.code
              ? <span style={{fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 600}}>{'<>'}</span>
              : <span className="emoji-ico">{app.emoji}</span>
            }
            <span className="tooltip">{app.label}{app.live ? '' : ' — coming soon'}</span>
          </button>
        ))}
      </div>
      <div className="ribbon-spacer" />
      <div className="ribbon-group">
        <button className="ribbon-btn" onClick={onOpenPlugins} title="Plugins">
          <span className="emoji-ico">🔌</span>
          <span className="tooltip">Plugin marketplace</span>
        </button>
        <button className="ribbon-btn" onClick={onOpenSettings} title="Settings">
          <span className="emoji-ico">⚙️</span>
          <span className="tooltip">Settings</span>
        </button>
      </div>
    </div>
  );
}

function LeftHeader({ collapsed, onToggle, search, setSearch, onNew, switcherStyle }) {
  if (collapsed) {
    return (
      <div className="left-header">
        <button className="icon-btn" onClick={onToggle} title="Expand sidebar">
          <Icon name="sidebar-left" size={15}/>
        </button>
      </div>
    );
  }
  return (
    <div className="left-header">
      <button className="icon-btn" onClick={onToggle} title="Collapse sidebar (⌘B)">
        <Icon name="sidebar-left" size={15}/>
      </button>
      <div className="search-wrap">
        <span className="search-icon"><Icon name="search" size={13}/></span>
        <input className="search-input" placeholder="Search" value={search} onChange={e => setSearch(e.target.value)}/>
      </div>
      <button className="icon-btn" onClick={onNew} title="New (⌘N)">
        <Icon name="plus" size={15}/>
      </button>
    </div>
  );
}

const SECTIONS = [
  { id: 'chat', label: 'Chat', icon: 'message-square' },
  { id: 'projects', label: 'Projects', icon: 'folder-kanban' },
  { id: 'memory', label: 'Memory', icon: 'brain' },
  { id: 'workflows', label: 'Workflows', icon: 'workflow' },
  { id: 'skills', label: 'Skills', icon: 'sparkles' },
];

function SectionSwitcher({ section, setSection, collapsed, style }) {
  return (
    <div className={`section-switcher style-${style}`}>
      {SECTIONS.map(s => {
        const active = s.id === section;
        return (
          <button key={s.id} className={'section-btn ' + (active ? 'active' : '')}
                  onClick={() => setSection(s.id)} title={s.label}>
            <Icon name={s.icon} size={15}/>
            {(active || style !== 'icons-text' || collapsed === false) && !collapsed &&
              <span className={'label ' + (style === 'icons-text' && !active ? 'hidden' : '')}>{s.label}</span>
            }
          </button>
        );
      })}
    </div>
  );
}

// -------- Chat section (pinned + grouped recents) --------
function ChatSection({ chats, activeChatId, onOpen, search }) {
  const q = search.toLowerCase();
  const filter = c => !q || c.title.toLowerCase().includes(q);
  const pinned = chats.filter(c => c.pinned).filter(filter);
  const recents = chats.filter(c => !c.pinned).filter(filter);
  const groups = [
    { id: 'today', label: 'Today' },
    { id: 'yesterday', label: 'Yesterday' },
    { id: 'this-week', label: 'This week' },
    { id: 'older', label: 'Older' },
  ];
  return (
    <div className="section-body">
      {pinned.length > 0 && (
        <div className="list-group">
          <div className="list-heading"><span>Pinned</span><span className="count">{pinned.length}</span></div>
          {pinned.map(c => (
            <div key={c.id} className={'list-row ' + (activeChatId === c.id ? 'active' : '')} onClick={() => onOpen(c.id)}>
              <Icon name="pin" size={12} className="row-icon"/>
              <span className="row-label">{c.title}</span>
              <span className="row-meta">{c.when}</span>
            </div>
          ))}
        </div>
      )}
      {groups.map(g => {
        const items = recents.filter(c => c.group === g.id);
        if (items.length === 0) return null;
        return (
          <div key={g.id} className="list-group">
            <div className="list-heading"><span>{g.label}</span><span className="count">{items.length}</span></div>
            {items.map(c => (
              <div key={c.id} className={'list-row ' + (activeChatId === c.id ? 'active' : '')} onClick={() => onOpen(c.id)}>
                <Icon name="message-square" size={13} className="row-icon"/>
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

// -------- Projects section --------
function ProjectsSection({ projects, chats, onOpen, activeChatId }) {
  const [open, setOpen] = useSt1(() => ({ [projects[0].id]: true, [projects[1].id]: true }));
  const toggle = id => setOpen(o => ({...o, [id]: !o[id]}));
  return (
    <div className="section-body">
      <div className="list-group">
        <div className="list-heading"><span>Projects</span><span className="count">{projects.length}</span></div>
        {projects.map(p => {
          const projChats = chats.filter(c => c.project === p.id);
          const projNotes = (p.notes || []).map(nid => NOTES[nid]).filter(Boolean);
          const o = open[p.id];
          return (
            <div key={p.id}>
              <div className={'list-row ' + (o ? 'open' : '')} onClick={() => toggle(p.id)}>
                <Icon name="chevron-right" size={12} className="chev"/>
                <Icon name="folder" size={13} className="row-icon"/>
                <span className="row-label">{p.name}</span>
                <span className="row-meta">{projChats.length + projNotes.length}</span>
              </div>
              {o && (
                <>
                  {projChats.map(c => (
                    <div key={c.id} className={'list-row nested ' + (activeChatId === c.id ? 'active' : '')} onClick={() => onOpen(c.id)}>
                      <Icon name="message-square" size={12} className="row-icon"/>
                      <span className="row-label">{c.title}</span>
                    </div>
                  ))}
                  {projNotes.map(n => (
                    <div key={n.id} className="list-row nested" onClick={() => onOpen(n.id, 'note')}>
                      <Icon name="file-text" size={12} className="row-icon"/>
                      <span className="row-label">{n.title}</span>
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

// -------- Memory section --------
function MemorySection({ onOpen, search }) {
  const q = search.toLowerCase();
  const all = Object.values(NOTES).filter(n => !q || n.title.toLowerCase().includes(q));
  const wiki = all.filter(n => n.kind === 'wiki');
  const notes = all.filter(n => n.kind === 'note');
  return (
    <div className="section-body">
      {wiki.length > 0 && (
        <div className="list-group">
          <div className="list-heading"><span>Wiki</span><span className="count">{wiki.length}</span></div>
          {wiki.map(n => (
            <div key={n.id} className="list-row" onClick={() => onOpen(n.id, 'note')}>
              <Icon name="book" size={13} className="row-icon"/>
              <span className="row-label">{n.title}</span>
            </div>
          ))}
        </div>
      )}
      <div className="list-group">
        <div className="list-heading"><span>Recent notes</span><span className="count">{notes.length}</span></div>
        {notes.map(n => (
          <div key={n.id} className="list-row" onClick={() => onOpen(n.id, 'note')}>
            <Icon name="file-text" size={13} className="row-icon"/>
            <span className="row-label">{n.title}</span>
            <span className="row-meta">{n.updated}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// -------- Workflows section --------
function WorkflowsSection({ onRun }) {
  return (
    <div className="section-body">
      <div className="list-heading"><span>Workflows</span><span className="count">{WORKFLOWS.length}</span></div>
      {WORKFLOWS.map(w => (
        <div key={w.id} className="workflow-row">
          <div className="wf-info">
            <div className="wf-name">{w.name}</div>
            <div className="wf-meta">{w.steps} steps · last run {w.lastRun}</div>
          </div>
          <button className="run-btn" onClick={() => onRun(w.name)}><Icon name="play" size={10}/></button>
          <button className="icon-btn tight"><Icon name="more" size={13}/></button>
        </div>
      ))}
    </div>
  );
}

// -------- Skills section --------
function SkillsSection({ skills, setSkills }) {
  const toggle = id => setSkills(skills.map(s => s.id === id ? {...s, on: !s.on} : s));
  const activeCount = skills.filter(s => s.on).length;
  return (
    <div className="section-body">
      <div className="list-heading"><span>Skills</span><span className="count">{activeCount} / {skills.length} on</span></div>
      {skills.map(s => (
        <div key={s.id} className="skill-row">
          <div className="skill-info">
            <div className="skill-name">{s.name}</div>
            <div className="skill-desc">{s.desc}</div>
          </div>
          <button className="icon-btn tight" title="Skill details"><Icon name="info" size={13}/></button>
          <button className={'toggle ' + (s.on ? 'on' : '')} onClick={() => toggle(s.id)} aria-label={s.on ? 'Disable skill' : 'Enable skill'}/>
        </div>
      ))}
    </div>
  );
}

function LeftFooter({ collapsed, onOpenSettings }) {
  return (
    <div className="left-footer">
      <div className="avatar">{USER.initials}</div>
      {!collapsed && <span className="user-name">{USER.fullName}</span>}
    </div>
  );
}

Object.assign(window, { Ribbon, LeftHeader, SectionSwitcher, ChatSection, ProjectsSection, MemorySection, WorkflowsSection, SkillsSection, LeftFooter });
