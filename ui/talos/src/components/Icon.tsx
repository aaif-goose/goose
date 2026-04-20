import type { CSSProperties } from "react";

export type IconName =
  | "chevron-right" | "chevron-down" | "chevron-up" | "chevron-left"
  | "x" | "search" | "plus"
  | "sidebar-left" | "sidebar-right" | "panel-toggle"
  | "send" | "arrow-up" | "paperclip" | "settings"
  | "message-square" | "mail" | "calendar" | "clipboard" | "code"
  | "folder" | "folder-kanban" | "brain" | "workflow" | "sparkles"
  | "zap" | "plug" | "file-text" | "book" | "pin" | "play"
  | "more" | "info" | "bug" | "at-sign" | "slash-command" | "model"
  | "box" | "pencil" | "copy" | "rotate" | "corner-down-left"
  | "check" | "scroll" | "sliders" | "globe";

interface IconProps {
  name: IconName;
  size?: number;
  style?: CSSProperties;
  className?: string;
}

export function Icon({ name, size = 16, style, className }: IconProps) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.75,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    style,
    className,
  };
  switch (name) {
    case "chevron-right": return <svg {...common}><polyline points="9 18 15 12 9 6"/></svg>;
    case "chevron-down": return <svg {...common}><polyline points="6 9 12 15 18 9"/></svg>;
    case "chevron-up": return <svg {...common}><polyline points="18 15 12 9 6 15"/></svg>;
    case "chevron-left": return <svg {...common}><polyline points="15 18 9 12 15 6"/></svg>;
    case "x": return <svg {...common}><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>;
    case "search": return <svg {...common}><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>;
    case "plus": return <svg {...common}><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>;
    case "sidebar-left":
    case "panel-toggle":
      return <svg {...common}><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/></svg>;
    case "sidebar-right": return <svg {...common}><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="15" y1="3" x2="15" y2="21"/></svg>;
    case "send": return <svg {...common}><path d="M22 2 11 13"/><path d="M22 2 15 22l-4-9-9-4 20-7Z"/></svg>;
    case "arrow-up": return <svg {...common}><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>;
    case "paperclip": return <svg {...common}><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>;
    case "settings": return <svg {...common}><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.59.86 1 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>;
    case "message-square": return <svg {...common}><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>;
    case "mail": return <svg {...common}><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg>;
    case "calendar": return <svg {...common}><rect x="3" y="4" width="18" height="18" rx="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>;
    case "clipboard": return <svg {...common}><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1"/></svg>;
    case "code": return <svg {...common}><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>;
    case "folder": return <svg {...common}><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>;
    case "folder-kanban": return <svg {...common}><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><path d="M8 11v5"/><path d="M12 11v3"/><path d="M16 11v4"/></svg>;
    case "brain": return <svg {...common}><path d="M9.5 2A2.5 2.5 0 0 1 12 4.5v15A2.5 2.5 0 0 1 9.5 22 2.5 2.5 0 0 1 7 19.5V17a2.5 2.5 0 0 1-2.5-2.5c0-.83.41-1.56 1.03-2A2.5 2.5 0 0 1 4.5 10a2.5 2.5 0 0 1 1.03-2A2.5 2.5 0 0 1 4.5 5.5 2.5 2.5 0 0 1 7 3V4.5A2.5 2.5 0 0 1 9.5 2z"/><path d="M14.5 2A2.5 2.5 0 0 0 12 4.5v15A2.5 2.5 0 0 0 14.5 22a2.5 2.5 0 0 0 2.5-2.5V17a2.5 2.5 0 0 0 2.5-2.5c0-.83-.41-1.56-1.03-2A2.5 2.5 0 0 0 19.5 10a2.5 2.5 0 0 0-1.03-2A2.5 2.5 0 0 0 19.5 5.5 2.5 2.5 0 0 0 17 3V4.5A2.5 2.5 0 0 0 14.5 2z"/></svg>;
    case "workflow": return <svg {...common}><rect x="3" y="3" width="6" height="6" rx="1"/><rect x="15" y="15" width="6" height="6" rx="1"/><path d="M6 9v6a2 2 0 0 0 2 2h7"/></svg>;
    case "sparkles": return <svg {...common}><path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3z"/><path d="M19 3v4"/><path d="M19 17v4"/><path d="M5 17v4"/></svg>;
    case "zap": return <svg {...common}><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>;
    case "plug": return <svg {...common}><path d="M9 2v6"/><path d="M15 2v6"/><path d="M6 10h12l-1 4a5 5 0 0 1-10 0z"/><path d="M12 18v4"/></svg>;
    case "file-text": return <svg {...common}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>;
    case "book": return <svg {...common}><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/></svg>;
    case "pin": return <svg {...common}><line x1="12" y1="17" x2="12" y2="22"/><path d="M5 17h14l-1.68-7.55A2 2 0 0 0 15.37 8H8.63a2 2 0 0 0-1.95 1.45z"/></svg>;
    case "play": return <svg {...common}><polygon points="5 3 19 12 5 21 5 3"/></svg>;
    case "more": return <svg {...common}><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>;
    case "info": return <svg {...common}><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>;
    case "bug": return <svg {...common}><rect x="8" y="6" width="8" height="14" rx="4"/><path d="M12 6V3"/><path d="M4 10h4"/><path d="M16 10h4"/><path d="M4 16h4"/><path d="M16 16h4"/><path d="M9 3l-1-1"/><path d="M15 3l1-1"/></svg>;
    case "at-sign": return <svg {...common}><circle cx="12" cy="12" r="4"/><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"/></svg>;
    case "slash-command": return <svg {...common}><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="16" x2="15" y2="8"/></svg>;
    case "model": return <svg {...common}><circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="8"/></svg>;
    case "box": return <svg {...common}><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/></svg>;
    case "pencil": return <svg {...common}><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4z"/></svg>;
    case "copy": return <svg {...common}><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>;
    case "rotate": return <svg {...common}><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>;
    case "corner-down-left": return <svg {...common}><polyline points="9 10 4 15 9 20"/><path d="M20 4v7a4 4 0 0 1-4 4H4"/></svg>;
    case "check": return <svg {...common}><polyline points="20 6 9 17 4 12"/></svg>;
    case "scroll": return <svg {...common}><path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-1"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M15 21h3a2 2 0 0 0 2-2v-3"/></svg>;
    case "sliders": return <svg {...common}><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/></svg>;
    case "globe": return <svg {...common}><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>;
    default: return <svg {...common}><circle cx="12" cy="12" r="8"/></svg>;
  }
}
