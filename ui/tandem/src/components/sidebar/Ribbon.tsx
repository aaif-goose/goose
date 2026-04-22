import { Icon } from "../Icon";

interface RibbonApp {
  id: string;
  emoji?: string;
  code?: boolean;
  label: string;
  live: boolean;
}

const RIBBON_APPS: RibbonApp[] = [
  { id: "chat", emoji: "💬", label: "Chat", live: true },
  { id: "email", emoji: "✉️", label: "Email", live: false },
  { id: "calendar", emoji: "📅", label: "Calendar", live: false },
  { id: "projects", emoji: "📋", label: "Projects", live: false },
  { id: "editor", code: true, label: "XML / JSON / Groovy", live: false },
];

interface RibbonProps {
  activeApp: string;
  onActivate: (id: string) => void;
  onPlaceholder: (name: string) => void;
  onOpenPlugins: () => void;
  onOpenSettings: () => void;
}

export function Ribbon({ activeApp, onActivate, onPlaceholder, onOpenPlugins, onOpenSettings }: RibbonProps) {
  return (
    <div className="ribbon">
      <div className="ribbon-group">
        {RIBBON_APPS.map((app) => (
          <button
            key={app.id}
            className={"ribbon-btn " + (activeApp === app.id ? "active" : "")}
            onClick={() => (app.live ? onActivate(app.id) : onPlaceholder(app.label))}
            title={app.label}
          >
            {app.code ? (
              <span style={{ fontFamily: "var(--font-mono)", fontSize: 13, fontWeight: 600 }}>{"<>"}</span>
            ) : (
              <span className="emoji-ico">{app.emoji}</span>
            )}
            <span className="tooltip">{app.label}{app.live ? "" : " — coming soon"}</span>
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

export function LeftFooter({ collapsed, initials, fullName }: { collapsed: boolean; initials: string; fullName: string }) {
  return (
    <div className="left-footer">
      <div className="avatar">{initials}</div>
      {!collapsed && <span className="user-name">{fullName}</span>}
    </div>
  );
}

export function LeftHeader({
  collapsed,
  onToggle,
  search,
  setSearch,
  onNew,
}: {
  collapsed: boolean;
  onToggle: () => void;
  search: string;
  setSearch: (v: string) => void;
  onNew: () => void;
}) {
  if (collapsed) {
    return (
      <div className="left-header">
        <button className="icon-btn" onClick={onToggle} title="Expand sidebar">
          <Icon name="sidebar-left" size={15} />
        </button>
      </div>
    );
  }
  return (
    <div className="left-header">
      <button className="icon-btn" onClick={onToggle} title="Collapse sidebar (⌘B)">
        <Icon name="sidebar-left" size={15} />
      </button>
      <div className="search-wrap">
        <span className="search-icon"><Icon name="search" size={13} /></span>
        <input className="search-input" placeholder="Search" value={search} onChange={(e) => setSearch(e.target.value)} />
      </div>
      <button className="icon-btn" onClick={onNew} title="New (⌘N)">
        <Icon name="plus" size={15} />
      </button>
    </div>
  );
}
