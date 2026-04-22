import { Icon } from "../Icon";
import { CONTEXT_FOLDERS, MODELS } from "../../data";
import type { McpServer } from "../../types";

const fmt = (n: number) =>
  n >= 1000 ? (n / 1000).toFixed(n >= 10000 ? 0 : 1).replace(/\.0$/, "") + "k" : String(n);

export function TokenPopover({
  used,
  max,
  autoCompactPct,
  onCompact,
  onChangeThreshold,
}: {
  used: number;
  max: number;
  autoCompactPct: number;
  onCompact: () => void;
  onChangeThreshold: (n: number) => void;
}) {
  const pct = Math.round((used / max) * 100);
  const totalDots = 60;
  const filledCount = Math.max(1, Math.round((pct / 100) * totalDots));
  const thresholdDot = Math.round((autoCompactPct / 100) * totalDots);
  const dots = Array.from({ length: totalDots }, (_, i) => {
    let cls = "dot";
    if (i === thresholdDot) cls = "dot threshold";
    else if (i < filledCount) cls = "dot filled";
    return <div key={i} className={cls} />;
  });
  return (
    <div
      className="popover token-popover"
      style={{ bottom: "calc(100% + 6px)", left: "50%", transform: "translateX(-50%)" }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="title">Context window</div>
      <div className="sub">
        Auto compact at {autoCompactPct}%
        <button title="Edit threshold">
          <Icon name="pencil" size={11} />
        </button>
      </div>
      <div className="dot-track">{dots}</div>
      <div className="legend">
        <span className="used">{fmt(used)} tokens · {pct}%</span>
        <span>{fmt(max)}</span>
      </div>
      <div className="actions">
        <button className="btn" onClick={onCompact}>
          <Icon name="scroll" size={13} /> Compact now
        </button>
        <button className="btn ghost" onClick={() => onChangeThreshold(autoCompactPct === 80 ? 70 : 80)}>
          <Icon name="sliders" size={13} /> Adjust threshold
        </button>
      </div>
    </div>
  );
}

export function ModelMenu({
  model,
  onPick,
  onClose,
}: {
  model: string;
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <div
      className="popover"
      style={{ bottom: "100%", left: 0, marginBottom: 8, minWidth: 320 }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="popover-header">Model</div>
      {MODELS.map((m) => (
        <button
          key={m.id}
          className="menu-item"
          onClick={() => {
            onPick(m.id);
            onClose();
          }}
        >
          <div style={{ flex: 1 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <span>{m.name}</span>
              {m.badge && (
                <span
                  style={{
                    fontSize: 10,
                    color: "var(--color-text-muted)",
                    fontFamily: "var(--font-mono)",
                  }}
                >
                  {m.badge}
                </span>
              )}
            </div>
            <div className="desc">
              {m.desc} · {(m.max / 1000).toFixed(0)}k ctx
            </div>
          </div>
          {model === m.id && <Icon name="check" size={14} className="check" />}
        </button>
      ))}
    </div>
  );
}

export function MCPMenu({
  servers,
  setServers,
  onClose: _onClose,
}: {
  servers: McpServer[];
  setServers: (next: McpServer[]) => void;
  onClose: () => void;
}) {
  const toggle = (id: string) => setServers(servers.map((s) => (s.id === id ? { ...s, on: !s.on } : s)));
  return (
    <div
      className="popover"
      style={{ bottom: "100%", left: 0, marginBottom: 8, minWidth: 280 }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="popover-header">MCP servers</div>
      {servers.map((s) => (
        <div key={s.id} className="menu-item" style={{ cursor: "default" }}>
          <div style={{ flex: 1 }}>
            <div>{s.name}</div>
            <div className="desc">{s.desc}</div>
          </div>
          <button className={"toggle " + (s.on ? "on" : "")} onClick={() => toggle(s.id)} />
        </div>
      ))}
      <div className="popover-sep" />
      <button className="menu-item"><Icon name="plus" size={13} /> Add server…</button>
    </div>
  );
}

export function ContextMenu({
  current,
  onPick,
  onClose,
}: {
  current: string;
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <div
      className="popover"
      style={{ bottom: "100%", left: 0, marginBottom: 8, minWidth: 260 }}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="popover-header">Context folder</div>
      {CONTEXT_FOLDERS.map((f) => (
        <button
          key={f.id}
          className="menu-item"
          onClick={() => {
            onPick(f.id);
            onClose();
          }}
        >
          <Icon
            name={f.kind === "default" ? "brain" : f.kind === "picker" ? "folder" : "folder-kanban"}
            size={14}
          />
          <span style={{ flex: 1 }}>{f.name}</span>
          {current === f.id && <Icon name="check" size={13} className="check" />}
        </button>
      ))}
    </div>
  );
}

