export function StatusBar({
  model,
  contextFolder,
  mcpActive,
  mcpTotal,
  sessionCount,
  skillsOn,
}: {
  model: string;
  contextFolder: string;
  mcpActive: number;
  mcpTotal: number;
  sessionCount: number;
  skillsOn: number;
}) {
  return (
    <div className="status-bar">
      <div className="item">
        <span className="sync-dot" />
        <span className="status-ok">Synced</span>
      </div>
      <div className="item">Folder: {contextFolder}</div>
      <div className="item">
        MCP: {mcpActive}/{mcpTotal}
      </div>
      <div className="item">Skills: {skillsOn} on</div>
      <div className="spacer" />
      <div className="item">{model}</div>
      <div className="item">{sessionCount} sessions</div>
      <div className="item">⌘K for commands</div>
    </div>
  );
}
