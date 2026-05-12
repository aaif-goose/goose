import React, {useCallback, useEffect, useState} from "react";
import {Box, Text, useInput, useStdout} from "ink";
import {TextInput} from "@inkjs/ui";
import type {GooseClient} from "@aaif/goose-sdk";
import {CRANBERRY, GOLD, RULE_COLOR, TEAL, TEXT_DIM, TEXT_PRIMARY} from "./colors.js";
import {Spinner, SPINNER_FRAMES} from "./components/Spinner.js";
import {ErrorScreen} from "./components/ErrorScreen.js";

type ExtEntry = {
  enabled: boolean;
  type: string;
  name: string;
  description: string;
  [key: string]: unknown;
};

function isExtEntry(v: unknown): v is ExtEntry {
  return !!v && typeof v === "object" && "enabled" in v && "type" in v && "name" in v
    && typeof (v as ExtEntry).enabled === "boolean"
    && typeof (v as ExtEntry).type === "string"
    && typeof (v as ExtEntry).name === "string";
}

type AddType = "stdio" | "streamable_http";
type Phase = "loading" | "list" | "add_type" | "add_value" | "add_name" | "add_desc" | "saving" | "error";

function deriveNameFromValue(addType: AddType, value: string): string {
  if (addType === "stdio") {
    const cmd = value.trim().split(/\s+/)[0] ?? "";
    return cmd.split("/").pop() ?? cmd;
  }
  try { return new URL(value.trim()).hostname; } catch { return value.trim(); }
}

function buildConfig(addType: AddType, value: string, name: string, description: string): ExtEntry {
  if (addType === "stdio") {
    const parts = value.trim().split(/\s+/);
    return {type: "stdio", enabled: true, name, description, cmd: parts[0] ?? "", args: parts.slice(1)};
  }
  return {type: "streamable_http", enabled: true, name, description, uri: value.trim()};
}

export default function ExtensionsManager({
  client,
  sessionId,
  height,
  onClose,
}: {
  client: GooseClient;
  sessionId: string;
  height: number;
  onClose: () => void;
}) {
  const {stdout} = useStdout();
  const columns = stdout?.columns ?? 80;

  const [phase, setPhase] = useState<Phase>("loading");
  const [spinIdx, setSpinIdx] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");
  const [entries, setEntries] = useState<ExtEntry[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);

  const [addType, setAddType] = useState<AddType>("stdio");
  const [addValue, setAddValue] = useState("");
  const [addName, setAddName] = useState("");
  const [addDesc, setAddDesc] = useState("");
  const [inputKey, setInputKey] = useState(0);

  useEffect(() => {
    const t = setInterval(() => setSpinIdx(i => (i + 1) % SPINNER_FRAMES.length), 300);
    return () => clearInterval(t);
  }, []);

  const reload = useCallback(async () => {
    setPhase("loading");
    try {
      const [configResp, sessionResp] = await Promise.all([
        client.goose.GooseConfigExtensions({}),
        client.goose.GooseSessionExtensions({sessionId}),
      ]);

      const allExtensions = (configResp.extensions as unknown[]).filter(isExtEntry);
      const activeNames = new Set(
        (sessionResp.extensions as Array<{name?: string}>).map(e => e.name),
      );

      setEntries(allExtensions.map(ext => ({...ext, enabled: activeNames.has(ext.name)})));
      setWarnings(configResp.warnings ?? []);
      setPhase("list");
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setPhase("error");
    }
  }, [client, sessionId]);

  useEffect(() => { reload(); }, [reload]);

  const withSaving = useCallback(async (fn: () => Promise<void>) => {
    setPhase("saving");
    try {
      await fn();
      await reload();
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setPhase("error");
    }
  }, [reload]);

  const toggleSelected = useCallback(() => {
    const sel = entries[selectedIdx];
    if (!sel) return;
    withSaving(async () => {
      if (sel.enabled) {
        await client.goose.GooseExtensionsRemove({sessionId, name: sel.name});
      } else {
        await client.goose.GooseExtensionsAdd({sessionId, config: sel as any});
      }
    });
  }, [entries, selectedIdx, client, sessionId, withSaving]);

  const saveNewExtension = useCallback((description: string) => {
    const config = buildConfig(addType, addValue, addName, description);
    withSaving(async () => {
      await client.goose.GooseConfigExtensionsAdd({
        name: config.name,
        extensionConfig: config as any,
        enabled: true,
      });
      await client.goose.GooseExtensionsAdd({sessionId, config: config as any});
    });
  }, [addType, addValue, addName, client, sessionId, withSaving]);

  useInput((ch, key) => {
    if (phase === "list") {
      if (key.escape) { onClose(); return; }
      if (key.upArrow) { setSelectedIdx(i => Math.max(i - 1, 0)); return; }
      if (key.downArrow) { setSelectedIdx(i => Math.min(i + 1, entries.length - 1)); return; }
      if (ch === " " || key.return) { toggleSelected(); return; }
      if (ch === "a") { setAddType("stdio"); setPhase("add_type"); return; }
    }
    if (phase === "add_type") {
      if (key.escape) { setPhase("list"); return; }
      if (key.upArrow || key.downArrow) { setAddType(t => t === "stdio" ? "streamable_http" : "stdio"); return; }
      if (key.return) { setAddValue(""); setInputKey(k => k + 1); setPhase("add_value"); return; }
    }
    if (key.escape) {
      if (phase === "add_value") { setPhase("add_type"); return; }
      if (phase === "add_name") { setInputKey(k => k + 1); setPhase("add_value"); return; }
      if (phase === "add_desc") { setInputKey(k => k + 1); setPhase("add_name"); return; }
    }
  });

  if (phase === "loading" || phase === "saving") {
    return (
      <Box flexDirection="column" height={height} width={columns} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Manage extensions ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>{phase === "loading" ? "Loading extensions…" : "Saving…"}</Text>
        </Box>
        <Box justifyContent="center" flexGrow={1} alignItems="center">
          <Spinner idx={spinIdx} />
        </Box>
      </Box>
    );
  }

  if (phase === "error") {
    return (
      <Box flexDirection="column" height={height} width={columns} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Manage extensions ◆</Text>
        </Box>
        <ErrorScreen errorMsg={errorMsg} onRetry={() => reload()} />
      </Box>
    );
  }

  const maxW = Math.min(columns - 4, 80);
  const inputW = Math.min(maxW - 10, 70);

  if (phase === "add_type") {
    const types: {value: AddType; label: string; hint: string}[] = [
      {value: "stdio", label: "Command (stdio)", hint: "run a local command"},
      {value: "streamable_http", label: "Endpoint (HTTP)", hint: "connect to a remote server"},
    ];
    return (
      <Box flexDirection="column" width={columns} height={height} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Add extension ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>Choose a connection type</Text>
        </Box>
        <Box justifyContent="center">
          <Box flexDirection="column">
            {types.map(t => {
              const active = addType === t.value;
              return (
                <Box key={t.value}>
                  <Text color={active ? GOLD : TEXT_DIM}>{active ? "▸ " : "  "}</Text>
                  <Text color={active ? TEXT_PRIMARY : TEXT_DIM} bold={active}>{t.label}</Text>
                  <Text color={TEXT_DIM}>  {t.hint}</Text>
                </Box>
              );
            })}
          </Box>
        </Box>
        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>↑↓ select · enter confirm · esc cancel</Text>
        </Box>
      </Box>
    );
  }

  if (phase === "add_value") {
    const isStdio = addType === "stdio";
    const placeholder = isStdio
      ? "npx -y @modelcontextprotocol/server-filesystem /tmp"
      : "http://localhost:8080/mcp";
    return (
      <Box flexDirection="column" width={columns} height={height} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ {isStdio ? "Enter command" : "Enter endpoint URL"} ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>{isStdio ? "The command to launch the extension" : "URL of the remote MCP server"}</Text>
        </Box>
        <Box justifyContent="center">
          <Box borderStyle="round" borderColor={RULE_COLOR} paddingX={2} width={inputW}>
            <Text color={CRANBERRY} bold>{"❯ "}</Text>
            <TextInput
              key={`value-${inputKey}`}
              placeholder={placeholder}
              onChange={setAddValue}
              onSubmit={(v) => {
                if (!v.trim()) return;
                setAddValue(v);
                setAddName(deriveNameFromValue(addType, v));
                setInputKey(k => k + 1);
                setPhase("add_name");
              }}
            />
          </Box>
        </Box>
        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>enter continue · esc back</Text>
        </Box>
      </Box>
    );
  }

  if (phase === "add_name") {
    return (
      <Box flexDirection="column" width={columns} height={height} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Name this extension ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>A short name to identify this extension</Text>
        </Box>
        <Box justifyContent="center">
          <Box borderStyle="round" borderColor={RULE_COLOR} paddingX={2} width={inputW}>
            <Text color={CRANBERRY} bold>{"❯ "}</Text>
            <TextInput
              key={`name-${inputKey}`}
              defaultValue={addName}
              placeholder="extension name"
              onChange={setAddName}
              onSubmit={(v) => {
                if (!v.trim()) return;
                setAddName(v.trim());
                setAddDesc("");
                setInputKey(k => k + 1);
                setPhase("add_desc");
              }}
            />
          </Box>
        </Box>
        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>enter continue · esc back</Text>
        </Box>
      </Box>
    );
  }

  if (phase === "add_desc") {
    return (
      <Box flexDirection="column" width={columns} height={height} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Description ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>What does this extension do? (optional)</Text>
        </Box>
        <Box justifyContent="center">
          <Box borderStyle="round" borderColor={RULE_COLOR} paddingX={2} width={inputW}>
            <Text color={CRANBERRY} bold>{"❯ "}</Text>
            <TextInput
              key={`desc-${inputKey}`}
              placeholder="what does this extension do?"
              onChange={setAddDesc}
              onSubmit={(v) => saveNewExtension(v.trim())}
            />
          </Box>
        </Box>
        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>enter save (leave empty to skip) · esc back</Text>
        </Box>
      </Box>
    );
  }

  const layoutW = maxW;
  const GUTTER = 2;
  const STATUS_W = 10;
  const nameW = Math.max(16, Math.floor(layoutW * 0.30));
  const descW = Math.max(8, layoutW - 2 - STATUS_W - nameW - 2 * GUTTER);

  const rows = Math.max(height - 9, 4);
  const maxStart = Math.max(0, entries.length - rows);
  const start = Math.min(maxStart, Math.max(0, selectedIdx - Math.floor(rows / 2)));
  const end = Math.min(entries.length, start + rows);
  const windowed = entries.slice(start, end);

  return (
    <Box flexDirection="column" width={columns} height={height} paddingX={2}>
      {/* Header */}
      <Box marginTop={1} />
      <Box justifyContent="center" marginBottom={1}>
        <Text color={TEXT_PRIMARY} bold>◆ Manage extensions ◆</Text>
      </Box>
      <Box justifyContent="center" marginBottom={2}>
        <Text color={TEXT_DIM}>Toggle, add, or remove extensions for this session</Text>
      </Box>

      {/* Extension List */}
      <Box flexDirection="column" flexGrow={1} justifyContent="flex-start">
        {entries.length === 0 ? (
          <Box justifyContent="center" alignItems="center" height={Math.max(rows - 1, 1)}>
            <Text color={TEXT_DIM}>No extensions configured — press a to add one</Text>
          </Box>
        ) : (
          <>
            {start > 0 && (
              <Box justifyContent="center" marginBottom={1}>
                <Text color={TEXT_DIM}>▲ {start} more above</Text>
              </Box>
            )}
            <Box justifyContent="center">
              <Box flexDirection="column" width={layoutW}>
                {windowed.map((ext, i) => {
                  const globalIdx = start + i;
                  const active = globalIdx === selectedIdx;
                  return (
                    <Box key={`${ext.type}:${ext.name}`} width={layoutW}>
                      <Text color={active ? GOLD : TEXT_DIM}>{active ? "▸ " : "  "}</Text>
                      <Box width={nameW}><Text color={active ? TEXT_PRIMARY : TEXT_DIM} bold={active} wrap="truncate">{ext.name}</Text></Box>
                      <Box width={GUTTER}><Text>{" ".repeat(GUTTER)}</Text></Box>
                      <Box width={descW}><Text color={TEXT_DIM} wrap="truncate">{ext.description || ""}</Text></Box>
                      <Box width={GUTTER}><Text>{" ".repeat(GUTTER)}</Text></Box>
                      <Box width={STATUS_W}><Text color={ext.enabled ? TEAL : TEXT_DIM} wrap="truncate">{ext.enabled ? "enabled" : "disabled"}</Text></Box>
                    </Box>
                  );
                })}
              </Box>
            </Box>
            {end < entries.length && (
              <Box justifyContent="center" marginTop={1}>
                <Text color={TEXT_DIM}>▼ {entries.length - end} more below</Text>
              </Box>
            )}
          </>
        )}
      </Box>

      {warnings.length > 0 && (
        <Box justifyContent="center" marginTop={1}>
          <Box width={layoutW} flexDirection="column">
            <Text color={GOLD}>Warnings</Text>
            {warnings.map((w, i) => (
              <Box key={i} width={layoutW}><Text color={TEXT_DIM} wrap="truncate">• {w}</Text></Box>
            ))}
          </Box>
        </Box>
      )}

      {/* Footer */}
      <Box justifyContent="center" marginTop={2}>
        <Text color={TEXT_DIM}>space/enter toggle · a add · esc back</Text>
      </Box>
    </Box>
  );
}
