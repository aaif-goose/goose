import { Icon } from "../Icon";
import type { ChatTab } from "../../types";

interface TabBarProps {
  tabs: ChatTab[];
  activeTab: string;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  onNew: () => void;
}

export function TabBar({ tabs, activeTab, onActivate, onClose, onNew }: TabBarProps) {
  return (
    <div className="tab-bar">
      <div className="tab-strip">
        {tabs.map((t) => (
          <div
            key={t.id}
            className={"tab " + (activeTab === t.id ? "active" : "")}
            onClick={() => onActivate(t.id)}
            onAuxClick={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                onClose(t.id);
              }
            }}
          >
            <Icon name="message-square" size={12} className="tab-icon" />
            <span className="tab-title">{t.title}</span>
            <button
              className="tab-close"
              onClick={(e) => {
                e.stopPropagation();
                onClose(t.id);
              }}
            >
              <Icon name="x" size={11} />
            </button>
          </div>
        ))}
        <button
          className="icon-btn"
          onClick={onNew}
          title="New tab (⌘N)"
          style={{ marginLeft: 4, alignSelf: "center" }}
        >
          <Icon name="plus" size={14} />
        </button>
      </div>
      <div className="tab-actions">
        <button className="icon-btn" title="Tab overflow">
          <Icon name="chevron-down" size={14} />
        </button>
      </div>
    </div>
  );
}
