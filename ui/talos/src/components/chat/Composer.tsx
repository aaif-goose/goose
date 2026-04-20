import { useEffect, useRef, useState } from "react";
import { Icon } from "../Icon";
import { CONTEXT_FOLDERS, MODELS } from "../../data";
import type { McpServer } from "../../types";
import { ContextMenu, MCPMenu, ModelMenu, TokenPopover } from "./popovers";

type PopoverKind = "model" | "mcp" | "context" | "token" | null;

export interface ComposerProps {
  value: string;
  setValue: (v: string) => void;
  onSend: () => void;
  attachments: string[];
  setAttachments: (next: string[]) => void;
  model: string;
  setModel: (id: string) => void;
  mcpServers: McpServer[];
  setMcpServers: (next: McpServer[]) => void;
  contextFolder: string;
  setContextFolder: (id: string) => void;
}

export function Composer({
  value,
  setValue,
  onSend,
  attachments,
  setAttachments,
  model,
  setModel,
  mcpServers,
  setMcpServers,
  contextFolder,
  setContextFolder,
}: ComposerProps) {
  const [open, setOpen] = useState<PopoverKind>(null);
  const [autoCompactPct, setAutoCompactPct] = useState(80);
  const ref = useRef<HTMLTextAreaElement>(null);
  const activeModel = MODELS.find((m) => m.id === model) || MODELS[0]!;
  const ctx = CONTEXT_FOLDERS.find((f) => f.id === contextFolder) || CONTEXT_FOLDERS[0]!;
  const activeMcpCount = mcpServers.filter((s) => s.on).length;
  const usedTokens = 14820;
  const pct = Math.round((usedTokens / activeModel.max) * 100);
  const fillClass = pct > 80 ? "danger" : pct > 60 ? "warn" : "";

  useEffect(() => {
    if (!open) return;
    const close = () => setOpen(null);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [open]);

  const handleKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (value.trim()) onSend();
    }
  };

  return (
    <div className="composer-wrap">
      <div className="composer">
        {attachments.length > 0 && (
          <div className="attachments">
            {attachments.map((a, i) => (
              <div key={i} className="attachment-chip">
                <Icon name="file-text" size={11} />
                <span>{a}</span>
                <button
                  className="remove"
                  onClick={() => setAttachments(attachments.filter((_, j) => j !== i))}
                >
                  <Icon name="x" size={10} />
                </button>
              </div>
            ))}
          </div>
        )}
        <textarea
          ref={ref}
          className="composer-input"
          placeholder="Ask anything, or paste a file…"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKey}
          rows={2}
        />
        <div className="composer-footer">
          {/* Context folder */}
          <div style={{ position: "relative" }}>
            <button
              className={"footer-btn " + (open === "context" ? "active" : "")}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(open === "context" ? null : "context");
              }}
            >
              <Icon
                name={ctx.kind === "default" ? "brain" : ctx.kind === "picker" ? "folder" : "folder-kanban"}
                size={13}
              />
              <span>{ctx.name}</span>
              <Icon name="chevron-down" size={11} className="chev" />
            </button>
            {open === "context" && (
              <ContextMenu current={contextFolder} onPick={setContextFolder} onClose={() => setOpen(null)} />
            )}
          </div>
          {/* Attach */}
          <button
            className="footer-btn"
            title="Attach file"
            onClick={() => setAttachments([...attachments, `file-${attachments.length + 1}.md`])}
          >
            <Icon name="paperclip" size={13} />
          </button>
          {/* Slash commands */}
          <button className="footer-btn" title="Slash commands (/)">
            <Icon name="slash-command" size={13} />
          </button>

          <div className="footer-spacer" />

          {/* Token counter */}
          <div style={{ position: "relative" }}>
            <button
              className={"footer-btn " + (open === "token" ? "active" : "")}
              onMouseEnter={() => setOpen("token")}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(open === "token" ? null : "token");
              }}
            >
              <div className="token-indicator">
                <div className="token-bar">
                  <div className={"token-bar-fill " + fillClass} style={{ width: pct + "%" }} />
                </div>
                <span className="kbd-hint">
                  {(usedTokens / 1000).toFixed(1)}k / {(activeModel.max / 1000).toFixed(0)}k
                </span>
              </div>
            </button>
            {open === "token" && (
              <div onMouseLeave={() => setOpen(null)}>
                <TokenPopover
                  used={usedTokens}
                  max={activeModel.max}
                  autoCompactPct={autoCompactPct}
                  onChangeThreshold={setAutoCompactPct}
                  onCompact={() => setOpen(null)}
                />
              </div>
            )}
          </div>

          <div className="footer-divider" />

          {/* Model selector */}
          <div style={{ position: "relative" }}>
            <button
              className={"footer-btn " + (open === "model" ? "active" : "")}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(open === "model" ? null : "model");
              }}
            >
              <Icon name="model" size={13} />
              <span>{activeModel.name.replace("Claude ", "")}</span>
              <Icon name="chevron-down" size={11} className="chev" />
            </button>
            {open === "model" && <ModelMenu model={model} onPick={setModel} onClose={() => setOpen(null)} />}
          </div>

          {/* MCP */}
          <div style={{ position: "relative" }}>
            <button
              className={"footer-btn " + (open === "mcp" ? "active" : "")}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(open === "mcp" ? null : "mcp");
              }}
            >
              <Icon name="plug" size={13} />
              <span>MCP</span>
              <span className="mcp-badge">{activeMcpCount}</span>
              <Icon name="chevron-down" size={11} className="chev" />
            </button>
            {open === "mcp" && (
              <MCPMenu servers={mcpServers} setServers={setMcpServers} onClose={() => setOpen(null)} />
            )}
          </div>

          {/* Bug report */}
          <button className="footer-btn" title="Report a bug">
            <Icon name="bug" size={13} />
          </button>
        </div>
      </div>
    </div>
  );
}
