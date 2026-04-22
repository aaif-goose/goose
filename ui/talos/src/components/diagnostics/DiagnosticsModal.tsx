import { useEffect, useState } from "react";
import { Icon } from "../Icon";
import type { ChatTab, McpServer } from "../../types";
import {
  buildEnabledMcpList,
  buildIssueUrl,
  defaultZipName,
  getSystemInfo,
  openIssueInBrowser,
  promptSaveZip,
  serializeTranscript,
  writeDiagnosticsZip,
} from "../../services/diagnostics";

interface DiagnosticsModalProps {
  open: boolean;
  onClose: () => void;
  currentTab: ChatTab;
  model: string;
  mcpServers: McpServer[];
  memoryDir?: string;
  onToast: (msg: string) => void;
}

export function DiagnosticsModal({
  open,
  onClose,
  currentTab,
  model,
  mcpServers,
  memoryDir,
  onToast,
}: DiagnosticsModalProps) {
  const [downloading, setDownloading] = useState(false);
  const [filingBug, setFilingBug] = useState(false);
  const [includeMemoryDir, setIncludeMemoryDir] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setError(null);
      setIncludeMemoryDir(false);
    }
  }, [open]);

  if (!open) return null;

  const handleDownload = async () => {
    setError(null);
    setDownloading(true);
    try {
      const outputPath = await promptSaveZip(defaultZipName(currentTab));
      if (!outputPath) {
        setDownloading(false);
        return;
      }
      const result = await writeDiagnosticsZip({
        sessionTranscriptJson: serializeTranscript(currentTab),
        sessionTitle: currentTab.title,
        sessionTabId: currentTab.id,
        provider: null,
        model,
        enabledMcpServers: buildEnabledMcpList(mcpServers),
        outputZipPath: outputPath,
        includeMemoryDir: includeMemoryDir && Boolean(memoryDir),
        memoryDir: memoryDir ?? null,
      });
      onToast(
        `Diagnostics saved (${formatBytes(result.bytesWritten)}) to ${result.outputPath}`,
      );
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setDownloading(false);
    }
  };

  const handleFileBug = async () => {
    setError(null);
    setFilingBug(true);
    try {
      const info = await getSystemInfo();
      info.provider = info.provider ?? null;
      info.model = info.model ?? model;
      info.enabledMcpServers =
        info.enabledMcpServers?.length > 0
          ? info.enabledMcpServers
          : buildEnabledMcpList(mcpServers);
      const url = buildIssueUrl(info);
      await openIssueInBrowser(url);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setFilingBug(false);
    }
  };

  const busy = downloading || filingBug;

  return (
    <div className="scrim" onClick={busy ? undefined : onClose}>
      <div
        className="palette"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 520, maxWidth: "90vw", padding: 0 }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "12px 16px",
            borderBottom: "1px solid var(--color-border)",
          }}
        >
          <Icon name="bug" size={14} />
          <div style={{ flex: 1, fontWeight: 600 }}>Report a problem</div>
          <button className="icon-btn tight" onClick={onClose} disabled={busy}>
            <Icon name="x" size={13} />
          </button>
        </div>

        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
          <div style={{ fontSize: "var(--text-sm)", color: "var(--color-text-secondary)" }}>
            Download a diagnostics bundle (ZIP) to attach to a bug report, or open a
            pre-filled GitHub issue in your browser.
          </div>

          <div className="diagnostics-list">
            <BundleItem label="Basic system info (OS, version, architecture, model)" />
            <BundleItem label="Current session transcript" />
            <BundleItem label="Talos settings snapshot" />
            <BundleItem label="UI state snapshot (composer drafts redacted)" />
            <BundleItem label="Recent Goose logs (tail of last 5)" />
            <BundleItem label="README explaining the bundle" />
          </div>

          <label
            style={{
              display: "flex",
              alignItems: "flex-start",
              gap: 8,
              fontSize: "var(--text-sm)",
              color: memoryDir ? "var(--color-text)" : "var(--color-text-muted)",
              cursor: memoryDir ? "pointer" : "not-allowed",
            }}
          >
            <input
              type="checkbox"
              checked={includeMemoryDir}
              disabled={!memoryDir || busy}
              onChange={(e) => setIncludeMemoryDir(e.target.checked)}
              style={{ marginTop: 3 }}
            />
            <span>
              Include memory folder contents
              <div style={{ fontSize: "var(--text-xs)", color: "var(--color-text-muted)" }}>
                {memoryDir
                  ? `Walks ${memoryDir} (text files only, capped at 50 MB). May contain personal notes.`
                  : "No memory folder configured. Set one in Settings to enable."}
              </div>
            </span>
          </label>

          <div
            style={{
              fontSize: "var(--text-xs)",
              color: "var(--color-text-muted)",
              padding: "8px 10px",
              borderRadius: "var(--radius-sm)",
              background: "var(--color-surface-sunken)",
              border: "1px solid var(--color-border-subtle)",
            }}
          >
            If your session contains sensitive information, do not share the
            diagnostics file publicly. When filing a bug, consider attaching the
            diagnostics ZIP to help triage.
          </div>

          {error && (
            <div style={{ color: "var(--color-danger)", fontSize: "var(--text-sm)" }}>
              {error}
            </div>
          )}

          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button className="btn ghost" onClick={onClose} disabled={busy}>
              Cancel
            </button>
            <button className="btn ghost" onClick={handleFileBug} disabled={busy}>
              {filingBug ? "Opening\u2026" : "File bug on GitHub"}
            </button>
            <button className="btn" onClick={handleDownload} disabled={busy}>
              {downloading ? "Saving\u2026" : "Download"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function BundleItem({ label, soon }: { label: string; soon?: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        fontSize: "var(--text-sm)",
        color: soon ? "var(--color-text-muted)" : "var(--color-text)",
      }}
    >
      <Icon name={soon ? "info" : "check"} size={13} />
      <span>{label}</span>
      {soon && (
        <span
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--color-text-muted)",
            marginLeft: "auto",
          }}
        >
          coming soon
        </span>
      )}
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
