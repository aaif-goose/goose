import { useState } from "react";
import { Icon, type IconName } from "../Icon";
import type { Command } from "../../types";

export function CommandPalette({
  open,
  onClose,
  commands,
  onRun,
}: {
  open: boolean;
  onClose: () => void;
  commands: Command[];
  onRun: (c: Command) => void;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  if (!open) return null;
  const filtered = commands.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()));
  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter" && filtered[sel]) {
      onRun(filtered[sel]!);
      onClose();
    }
  };
  return (
    <div className="scrim" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <div className="palette-input-wrap">
          <span className="search-ico"><Icon name="search" size={15} /></span>
          <input
            className="palette-input"
            placeholder="Type a command or search…"
            autoFocus
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={onKey}
          />
        </div>
        <div className="palette-results">
          {filtered.map((c, i) => (
            <button
              key={i}
              className={"palette-row " + (i === sel ? "active" : "")}
              onClick={() => {
                onRun(c);
                onClose();
              }}
              onMouseEnter={() => setSel(i)}
            >
              <Icon name={c.icon as IconName} size={14} className="ico" />
              <span className="label">{c.label}</span>
              {c.section && <span className="section">{c.section}</span>}
              {c.kbd && <kbd style={{ marginLeft: 8 }}>{c.kbd}</kbd>}
            </button>
          ))}
          {filtered.length === 0 && (
            <div
              style={{
                padding: 24,
                textAlign: "center",
                color: "var(--color-text-muted)",
                fontSize: 13,
              }}
            >
              No matching command
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
