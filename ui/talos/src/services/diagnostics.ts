import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ChatTab, McpServer } from "../types";

export interface SystemInfo {
  appVersion: string;
  appName: string;
  tauriVersion: string;
  os: string;
  osVersion: string;
  architecture: string;
  timestampUtc: string;
  provider: string | null;
  model: string | null;
  enabledMcpServers: string[];
}

export interface DiagnosticsRequest {
  sessionTranscriptJson: string;
  sessionTitle: string;
  sessionTabId: string;
  provider: string | null;
  model: string | null;
  enabledMcpServers: string[];
  outputZipPath: string;
  includeMemoryDir: boolean;
}

export interface WriteDiagnosticsResult {
  bytesWritten: number;
  entries: string[];
  outputPath: string;
}

const GITHUB_ISSUES_NEW =
  "https://github.com/larmax82/goose/issues/new";

export async function getSystemInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("get_system_info");
}

export async function writeDiagnosticsZip(
  request: DiagnosticsRequest,
): Promise<WriteDiagnosticsResult> {
  return invoke<WriteDiagnosticsResult>("write_diagnostics_zip", { request });
}

export async function promptSaveZip(defaultName: string): Promise<string | null> {
  const picked = await save({
    defaultPath: defaultName,
    filters: [{ name: "Zip archive", extensions: ["zip"] }],
  });
  return typeof picked === "string" ? picked : null;
}

/**
 * Strip UI-only fields from messages before writing to disk.
 */
export function serializeTranscript(tab: ChatTab): string {
  const cleaned = tab.messages.map((m) => ({
    id: m.id,
    role: m.role,
    model: m.model,
    paragraphs: m.paragraphs,
    bullets: m.bullets,
    followup: m.followup,
    tool: m.tool,
    tools: m.tools,
  }));
  return JSON.stringify(
    {
      tabId: tab.id,
      title: tab.title,
      gooseSessionId: tab.gooseSessionId ?? null,
      messages: cleaned,
    },
    null,
    2,
  );
}

export function defaultZipName(tab: ChatTab): string {
  const now = new Date();
  const pad = (n: number) => n.toString().padStart(2, "0");
  const stamp =
    now.getFullYear().toString() +
    pad(now.getMonth() + 1) +
    pad(now.getDate()) +
    "-" +
    pad(now.getHours()) +
    pad(now.getMinutes()) +
    pad(now.getSeconds());
  const shortId = tab.id.slice(-6);
  return `talos-diagnostics-${stamp}-${shortId}.zip`;
}

export function buildEnabledMcpList(servers: McpServer[]): string[] {
  return servers.filter((s) => s.on).map((s) => s.name);
}

export function buildIssueUrl(
  info: SystemInfo,
  providerFallback = "[e.g. openrouter - claude-opus-4.7]",
  mcpFallback = "[e.g. filesystem, github]",
): string {
  const providerModel =
    info.provider || info.model
      ? [info.provider, info.model].filter(Boolean).join(" - ")
      : providerFallback;
  const mcpList =
    info.enabledMcpServers.length > 0
      ? info.enabledMcpServers.join(", ")
      : mcpFallback;

  const body =
    `**Describe the bug**\n\n` +
    `Before filing, please:\n` +
    `- Download the Talos diagnostics zip from the same dialog and attach it below.\n` +
    `- Check common issues: https://github.com/larmax82/goose/issues?q=is%3Aissue\n\n` +
    `A clear and concise description of what the bug is.\n\n` +
    `---\n\n` +
    `**To Reproduce**\n` +
    `Steps to reproduce the behavior:\n` +
    `1. Go to '...'\n` +
    `2. Click on '....'\n` +
    `3. Scroll down to '....'\n` +
    `4. See error\n\n` +
    `---\n\n` +
    `**Expected behavior**\n` +
    `A clear and concise description of what you expected to happen.\n\n` +
    `---\n\n` +
    `**Screenshots**\n` +
    `If applicable, add screenshots to help explain your problem.\n\n` +
    `---\n\n` +
    `**Environment**\n` +
    `- **App:** Talos ${info.appVersion} (Tauri ${info.tauriVersion})\n` +
    `- **OS & Arch:** ${info.os} ${info.osVersion} ${info.architecture}\n` +
    `- **Provider & Model:** ${providerModel}\n` +
    `- **MCP servers enabled:** ${mcpList}\n\n` +
    `---\n\n` +
    `**Additional context**\n` +
    `Add any other context about the problem here.\n\n` +
    `Please attach the diagnostics zip you just downloaded — it contains a snapshot ` +
    `of the current session transcript and environment details which will speed up ` +
    `triage significantly.\n`;

  const params = new URLSearchParams();
  params.set("template", "bug_report.md");
  params.set("labels", "bug,talos");
  params.set("body", body);
  return `${GITHUB_ISSUES_NEW}?${params.toString()}`;
}

export async function openIssueInBrowser(url: string): Promise<void> {
  await openUrl(url);
}
