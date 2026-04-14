#!/usr/bin/env node
import React, { useState, useEffect, useCallback, useRef } from "react";
import { Box, Text, render, useApp, useInput, useStdout } from "ink";
import { MultilineInput } from "ink-multiline-input";
import meow from "meow";
import { spawn } from "node:child_process";
import { Readable, Writable } from "node:stream";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type {
  SessionNotification,
  RequestPermissionRequest,
  RequestPermissionResponse,
  Stream,
  ContentChunk,
  ToolCall,
  ToolCallUpdate,
} from "@agentclientprotocol/sdk";
import { ndJsonStream } from "@agentclientprotocol/sdk";
import { GooseClient } from "@aaif/goose-sdk";
import Onboarding from "./onboarding.js";
import ConfigureScreen, { ConfigureIntent } from "./configure.js";
import ExtensionsManager from "./extensions.js";
import type { PendingPermission, ResponseItem, Turn } from "./types.js";
import {
  emptyLine,
  renderUserPrompt,
  renderToolCallItem,
  renderErrorItem,
  renderContentItem,
  renderLoadingIndicator,
  renderQueuedMessages,
} from "./components/ContentRenderers.js";
import { Header } from "./components/Header.js";
import { Rule } from "./components/Rule.js";
import { isErrorStatus, formatError } from "./utils.js";
import { CRANBERRY, TEAL, GOLD, TEXT_PRIMARY, TEXT_SECONDARY, TEXT_DIM, RULE_COLOR } from "./colors.js";
import { Spinner, SPINNER_FRAMES } from "./components/Spinner.js";
import {
  PASTE_THRESHOLD,
  INPUT_MAX_ROWS,
  SENT_PREVIEW_LEN,
  GOOSE_FRAMES,
  INITIAL_GREETING,
  PERMISSION_LABELS,
  PERMISSION_KEYS,
} from "./constants.js";

const InputBar = React.memo(function InputBar({
  width,
  input,
  onChange,
  onSubmit,
  queued,
  scrollHint,
  placeholder,
  focused,
  pastedFull,
  onPastedFullChange,
}: {
  width: number;
  input: string;
  onChange: (v: string) => void;
  onSubmit: (v: string) => void;
  queued: boolean;
  scrollHint: boolean;
  placeholder?: string;
  focused: boolean;
  pastedFull: string | null;
  onPastedFullChange: (v: string | null) => void;
}) {
  const prevLenRef = useRef(input.length);

  const handleChange = useCallback(
    (newValue: string) => {
      const delta = newValue.length - prevLenRef.current;
      prevLenRef.current = newValue.length;
      if (delta >= PASTE_THRESHOLD) {
        onPastedFullChange(newValue);
        onChange(newValue);
      } else {
        if (pastedFull !== null) onPastedFullChange(null);
        onChange(newValue);
      }
    },
    [onChange, pastedFull, onPastedFullChange],
  );

  const handleSubmit = useCallback(
    (value: string) => {
      prevLenRef.current = 0;
      onPastedFullChange(null);
      onSubmit(value);
    },
    [onSubmit, onPastedFullChange],
  );

  useInput(
    (ch, key) => {
      if (key.return) {
        handleSubmit(input);
        return;
      }
      if (key.backspace || key.delete) {
        prevLenRef.current = 0;
        onPastedFullChange(null);
        onChange("");
        return;
      }
      if (key.escape) {
        prevLenRef.current = 0;
        onPastedFullChange(null);
        onChange("");
        return;
      }
      if (ch && !key.ctrl && !key.meta) {
        prevLenRef.current = ch.length;
        onPastedFullChange(null);
        onChange(ch);
      }
    },
    { isActive: focused && pastedFull !== null },
  );

  const isPasteMode = pastedFull !== null;
  const constrainedWidth = Math.max(width, 20);
  const contentWidth = Math.max(constrainedWidth - 6, 10);

  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor={RULE_COLOR}
      paddingX={1}
      width={constrainedWidth}
      flexShrink={0}
    >
      <Box>
        <Text color={CRANBERRY} bold>{"❯ "}</Text>
        {isPasteMode ? (
          <Box width={contentWidth} justifyContent="space-between">
            <Box width={Math.max(contentWidth - 20, 10)}>
              <Text color={TEXT_PRIMARY} wrap="truncate-end">
                {(() => {
                  const text = pastedFull;
                  const availableWidth = Math.max(contentWidth - 20, 10);
                  const flat = text.replace(/\n/g, " ").replace(/\s+/g, " ").trim();
                  if (flat.length <= availableWidth) return flat;
                  const suffix = ` (${flat.length.toLocaleString()} chars)`;
                  const previewLen = Math.max(availableWidth - suffix.length - 1, 5);
                  return flat.slice(0, previewLen) + "…" + suffix;
                })()}
              </Text>
            </Box>
            {scrollHint && <Text color={TEXT_DIM}>shift+↑↓ history</Text>}
          </Box>
        ) : (
          <Box flexGrow={1} justifyContent="space-between">
            <MultilineInput
              value={input}
              onChange={handleChange}
              onSubmit={handleSubmit}
              rows={1}
              maxRows={INPUT_MAX_ROWS}
              placeholder={placeholder}
              focus={focused}
              keyBindings={{
                submit: (key) => key.return && !key.ctrl,
                newline: (key) => key.return && key.ctrl,
              }}
              useCustomInput={(handler, isActive) => {
                useInput((ch, key) => {
                  if (key.shift && (key.upArrow || key.downArrow)) return;
                  handler(ch, key);
                }, { isActive });
              }}
            />
            {scrollHint && <Text color={TEXT_DIM}>shift+↑↓ history</Text>}
          </Box>
        )}
      </Box>
      {isPasteMode && (
        <Box>
          <Text color={TEXT_DIM} italic>
            enter to send · esc to clear
          </Text>
        </Box>
      )}
      {queued && (
        <Box>
          <Text color={GOLD} dimColor italic>
            message queued — will send when goose finishes
          </Text>
        </Box>
      )}
    </Box>
  );
});

function buildContentLines({
  turn,
  turnIndex,
  width,
  loading,
  status,
  spinIdx,
  pendingPermission,
  permissionIdx,
  toolCallsExpanded,
  queuedMessages,
}: {
  turn: Turn | undefined;
  turnIndex: number;
  width: number;
  loading: boolean;
  status: string;
  spinIdx: number;
  pendingPermission: PendingPermission | null;
  permissionIdx: number;
  toolCallsExpanded: boolean;
  queuedMessages: string[];
}): React.ReactElement[] {
  const lines: React.ReactElement[] = [];
  if (!turn) return lines;

  const safeWidth = Math.max(width, 20);

  const turnId = String(turnIndex);
  lines.push(...renderUserPrompt(turn.userText, safeWidth, turnId, (text: string, availableWidth: number) => {
    const flat = text.replace(/\n/g, " ").replace(/\s+/g, " ").trim();
    const safeWidth = Math.max(availableWidth, 10);
    const maxPreview = Math.max(safeWidth - 30, Math.min(SENT_PREVIEW_LEN, safeWidth - 10));
    if (flat.length <= maxPreview + 10) {
      return (
        <Box width={safeWidth}>
          <Text color={TEXT_PRIMARY} bold wrap="wrap">{flat}</Text>
        </Box>
      );
    }
    const preview = flat.slice(0, maxPreview) + "…";
    const remaining = flat.length - maxPreview;
    return (
      <Box width={safeWidth}>
        <Text color={TEXT_PRIMARY} bold wrap="wrap">{preview}</Text>
        <Text color={TEXT_DIM}> ({remaining.toLocaleString()} more chars)</Text>
      </Box>
    );
  }));

  // Process response items
  const hasToolCalls = turn.responseItems.some((it) => it.itemType === "tool_call");
  let tcIdx = 0;

  for (let i = 0; i < turn.responseItems.length; i++) {
    const item = turn.responseItems[i]!;

    if (item.itemType === "tool_call") {
      lines.push(...renderToolCallItem(item, i, safeWidth, toolCallsExpanded, tcIdx === 0, hasToolCalls));
      tcIdx++;
    } else if (item.itemType === "error") {
      lines.push(...renderErrorItem(item, i, safeWidth));
    } else if (item.itemType === "content_chunk") {
      lines.push(...renderContentItem(item, i, safeWidth));
    }
  }

  // Loading indicator
  if (loading && !pendingPermission) {
    lines.push(...renderLoadingIndicator(status, spinIdx, safeWidth));
  }

  // Permission dialog
  if (pendingPermission) {
    const perm = pendingPermission;
    const selectedIdx = permissionIdx;
    const fullWidth = safeWidth;
    const dialogWidth = Math.min(fullWidth - 2, 58);
    const innerWidth = Math.max(dialogWidth - 4, 10);
    const hRule = "─".repeat(Math.max(dialogWidth - 2, 0));
    const permissionLines: React.ReactElement[] = [];

    permissionLines.push(emptyLine(`pm-gap-${perm.toolTitle.slice(0, 10).replace(/[^a-zA-Z0-9]/g, '')}`, fullWidth));

    permissionLines.push(
      <Box key="pm-t" width={fullWidth} height={1}>
        <Text color={GOLD}>╭{hRule}╮</Text>
      </Box>,
    );

    const row = (key: string, content: React.ReactNode) => {
      permissionLines.push(
        <Box key={key} width={fullWidth} height={1}>
          <Text color={GOLD}>│ </Text>
          <Box width={innerWidth} height={1}>{content}</Box>
          <Text color={GOLD}> │</Text>
        </Box>,
      );
    };

    row("pm-title", <Text color={GOLD} bold>🔒 Permission required</Text>);
    row("pm-g1", <Text> </Text>);
    row("pm-tool", <Text wrap="truncate-end" color={TEXT_PRIMARY}>{perm.toolTitle}</Text>);
    row("pm-g2", <Text> </Text>);

    for (let i = 0; i < perm.options.length; i++) {
      const opt = perm.options[i]!;
      const k = PERMISSION_KEYS[opt.kind] ?? String(i + 1);
      const label = PERMISSION_LABELS[opt.kind] ?? opt.name;
      const active = i === selectedIdx;
      row(`pm-o${i}`, (
        <>
          <Text color={active ? GOLD : RULE_COLOR}>{active ? "▸ " : "  "}</Text>
          <Text color={active ? TEXT_PRIMARY : TEXT_SECONDARY} bold={active}>
            [{k}] {label}
          </Text>
        </>
      ));
    }

    row("pm-g3", <Text> </Text>);
    row("pm-help", <Text color={TEXT_DIM}>↑↓ select · enter confirm · esc cancel</Text>);

    permissionLines.push(
      <Box key="pm-b" width={fullWidth} height={1}>
        <Text color={GOLD}>╰{hRule}╯</Text>
      </Box>,
    );

    lines.push(...permissionLines);
  }

  // Queued messages
  lines.push(...renderQueuedMessages(queuedMessages, safeWidth));

  return lines;
}

const Viewport = React.memo(function Viewport({
  lines,
  height,
  width,
  scrollOffset,
}: {
  lines: React.ReactElement[];
  height: number;
  width: number;
  scrollOffset: number;
}) {
  const total = lines.length;
  const overflows = total > height;

  const contentHeight = overflows ? Math.max(height - 2, 1) : height;

  const maxEnd = total;
  const minEnd = Math.min(contentHeight, total);
  const endIdx = Math.max(minEnd, Math.min(maxEnd - scrollOffset, maxEnd));
  const startIdx = Math.max(0, endIdx - contentHeight);

  const visible = lines.slice(startIdx, endIdx);

  const padCount = contentHeight - visible.length;

  const elements: React.ReactElement[] = [];

  if (overflows) {
    const above = startIdx;
    elements.push(
      <Box key="si-up" width={width} height={1} justifyContent="center">
        {above > 0
          ? <Text color={TEXT_DIM}>▲ {above} more (↑)</Text>
          : <Text> </Text>}
      </Box>,
    );
  }

  for (let i = 0; i < padCount; i++) {
    elements.push(emptyLine(`vp-pad-${i}`, width));
  }
  elements.push(...visible);

  if (overflows) {
    const below = total - endIdx;
    elements.push(
      <Box key="si-dn" width={width} height={1} justifyContent="center">
        {below > 0
          ? <Text color={TEXT_DIM}>▼ {below} more (↓)</Text>
          : <Text> </Text>}
      </Box>,
    );
  }

  const constrainedWidth = Math.max(width, 10);
  const constrainedHeight = Math.max(height, 1);

  return (
    <Box flexDirection="column" height={constrainedHeight} width={constrainedWidth}>
      {elements}
    </Box>
  );
});

const SplashScreen = React.memo(function SplashScreen({
  animFrame,
  width,
  height,
  status,
  loading,
  spinIdx,
}: {
  animFrame: number;
  width: number;
  height: number;
  status: string;
  loading: boolean;
  spinIdx: number;
}) {
  const frame = GOOSE_FRAMES[animFrame % GOOSE_FRAMES.length]!;
  const statusColor =
    status === "ready" ? TEAL : isErrorStatus(status) ? CRANBERRY : TEXT_DIM;

  const contentHeight = frame.length + 1 + 1 + 1 + 2 + 1;

  const topPad = Math.max(0, Math.floor((height - contentHeight) / 2));

  // Use original dimensions for outer container to maintain centering
  const safeWidth = Math.max(width, 20);
  const safeHeight = Math.max(height, 10);

  return (
    <Box
      flexDirection="column"
      alignItems="center"
      width={safeWidth}
      height={safeHeight}
      overflow="hidden"
    >
      {topPad > 0 && <Box height={topPad} />}
      <Box flexDirection="column" alignItems="center">
        {frame.map((line, i) => (
          <Text key={i} color={TEXT_PRIMARY}>{line}</Text>
        ))}
      </Box>
      <Box marginTop={1}>
        <Text color={TEXT_PRIMARY} bold>goose</Text>
      </Box>
      <Box alignItems="center">
        <Text color={TEXT_DIM}>your on-machine AI agent</Text>
      </Box>
      <Box marginTop={2} gap={1} alignItems="center">
        {loading && <Spinner idx={spinIdx} />}
        <Text color={statusColor}>{status}</Text>
      </Box>
    </Box>
  );
});

function App({
  serverConnection,
  initialPrompt,
}: {
  serverConnection: Stream | string;
  initialPrompt?: string;
}) {
  const { exit } = useApp();
  const { stdout } = useStdout();
  const termWidth = stdout?.columns ?? 80;
  const termHeight = stdout?.rows ?? 24;

  const [turns, setTurns] = useState<Turn[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState("connecting…");
  const [spinIdx, setSpinIdx] = useState(0);
  const [gooseFrame, setGooseFrame] = useState(0);
  const [bannerVisible, setBannerVisible] = useState(true);
  const [pendingPermission, setPendingPermission] =
    useState<PendingPermission | null>(null);
  const [permissionIdx, setPermissionIdx] = useState(0);
  const [queuedMessages, setQueuedMessages] = useState<string[]>([]);

  const [viewTurnIdx, setViewTurnIdx] = useState(-1);
  const [toolCallsExpanded, setToolCallsExpanded] = useState(false);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [pastedFull, setPastedFull] = useState<string | null>(null);
  const [needsOnboarding, setNeedsOnboarding] = useState(false);
  type Overlay = { screen: "configure"; intent: ConfigureIntent } | { screen: "extensions" };
  const [overlay, setOverlay] = useState<Overlay | null>(null);

  const clientRef = useRef<GooseClient | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const streamBuf = useRef("");
  const sentInitialPrompt = useRef(false);
  const queueRef = useRef<string[]>([]);
  const isProcessingRef = useRef(false);

  useEffect(() => {
    const t = setInterval(() => {
      setSpinIdx((i) => (i + 1) % SPINNER_FRAMES.length);
      setGooseFrame((f) => f + 1);
    }, 300);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    if (turns.length > 0) setBannerVisible(false);
  }, [turns]);

  useEffect(() => {
    setToolCallsExpanded(false);
    setScrollOffset(0);
  }, [viewTurnIdx, turns.length]);

  const appendAgent = useCallback((text: string) => {
    setTurns((prev) => {
      if (prev.length === 0) return prev;
      const last = { ...prev[prev.length - 1]! };
      const newItems = [...last.responseItems];

      if (
        newItems.length > 0 &&
        newItems[newItems.length - 1]!.itemType === "content_chunk"
      ) {
        const lastItem = newItems[newItems.length - 1] as ContentChunk & {
          itemType: "content_chunk";
        };
        if (lastItem.content.type === "text") {
          newItems[newItems.length - 1] = {
            ...lastItem,
            content: { ...lastItem.content, text: lastItem.content.text + text },
          };
        } else {
          newItems.push({ itemType: "content_chunk", content: { type: "text", text } });
        }
      } else {
        newItems.push({ itemType: "content_chunk", content: { type: "text", text } });
      }

      return [...prev.slice(0, -1), { ...last, responseItems: newItems }];
    });
  }, []);

  const appendError = useCallback((errorMessage: string) => {
    setTurns((prev) => {
      if (prev.length === 0) return prev;
      const last = { ...prev[prev.length - 1]! };
      const newItems = [...last.responseItems];
      newItems.push({ itemType: "error", message: errorMessage });
      return [...prev.slice(0, -1), { ...last, responseItems: newItems }];
    });
  }, []);

  const handleToolCall = useCallback((tc: ToolCall) => {
    setTurns((prev) => {
      if (prev.length === 0) return prev;
      const last = { ...prev[prev.length - 1]! };
      const newItems = [...last.responseItems];
      const newById = new Map(last.toolCallsById);
      const index = newItems.length;
      newItems.push({ ...tc, itemType: "tool_call" });
      newById.set(tc.toolCallId, index);
      return [
        ...prev.slice(0, -1),
        { ...last, responseItems: newItems, toolCallsById: newById },
      ];
    });
  }, []);

  const handleToolCallUpdate = useCallback((update: ToolCallUpdate) => {
    setTurns((prev) => {
      if (prev.length === 0) return prev;
      const last = { ...prev[prev.length - 1]! };
      const index = last.toolCallsById.get(update.toolCallId);
      if (index === undefined) return prev;
      const item = last.responseItems[index];
      if (!item || item.itemType !== "tool_call") return prev;
      const updated: ToolCall & { itemType: "tool_call" } = { ...item };
      if (update.title != null) updated.title = update.title;
      if (update.status != null) updated.status = update.status;
      if (update.kind != null) updated.kind = update.kind;
      if (update.rawInput !== undefined) updated.rawInput = update.rawInput;
      if (update.rawOutput !== undefined) updated.rawOutput = update.rawOutput;
      if (update.content != null) updated.content = update.content;
      if (update.locations != null) updated.locations = update.locations;
      const newItems = [...last.responseItems];
      newItems[index] = updated;
      return [...prev.slice(0, -1), { ...last, responseItems: newItems }];
    });
  }, []);

  const addUserTurn = useCallback((text: string) => {
    setTurns((prev) => [
      ...prev,
      { userText: text, responseItems: [], toolCallsById: new Map() },
    ]);
    setViewTurnIdx(-1);
    setToolCallsExpanded(false);
    setScrollOffset(0);
  }, []);

  const resolvePermission = useCallback(
    (option: { optionId: string } | "cancelled") => {
      if (!pendingPermission) return;
      const { resolve } = pendingPermission;
      if (option === "cancelled") {
        resolve({ outcome: { outcome: "cancelled" } });
      } else {
        resolve({ outcome: { outcome: "selected", optionId: option.optionId } });
      }
      setPendingPermission(null);
      setPermissionIdx(0);
    },
    [pendingPermission],
  );

  const executePrompt = useCallback(
    async (text: string) => {
      const client = clientRef.current;
      const sid = sessionIdRef.current;
      if (!client || !sid) return;

      addUserTurn(text);
      setLoading(true);
      setStatus("thinking…");
      streamBuf.current = "";

      try {
        const result = await client.prompt({
          sessionId: sid,
          prompt: [{ type: "text", text }],
        });
        if (streamBuf.current) appendAgent("");
        setStatus(
          result.stopReason === "end_turn"
            ? "ready"
            : `stopped: ${result.stopReason}`,
        );
      } catch (e: unknown) {
        const errorMsg = formatError(e);
        setStatus(`error`);
        appendError(errorMsg);
      } finally {
        setLoading(false);
      }
    },
    [appendAgent, appendError, addUserTurn],
  );

  const processQueue = useCallback(async () => {
    if (isProcessingRef.current) return;
    isProcessingRef.current = true;
    while (queueRef.current.length > 0) {
      const next = queueRef.current.shift()!;
      setQueuedMessages([...queueRef.current]);
      await executePrompt(next);
    }
    isProcessingRef.current = false;
  }, [executePrompt]);

  const sendPrompt = useCallback(
    async (text: string) => {
      await executePrompt(text);
      if (queueRef.current.length > 0) processQueue();
    },
    [executePrompt, processQueue],
  );

  const createSession = useCallback(async (client: GooseClient) => {
    setStatus("creating session…");
    setLoading(true);
    try {
      const session = await client.newSession({
        cwd: process.cwd(),
        mcpServers: [],
      });
      sessionIdRef.current = session.sessionId;
      setLoading(false);
      setStatus("ready");

      if (initialPrompt && !sentInitialPrompt.current) {
        sentInitialPrompt.current = true;
        await sendPrompt(initialPrompt);
        setTimeout(() => exit(), 100);
      }
    } catch (e: unknown) {
      const errorMsg = formatError(e);
      setStatus(`failed: ${errorMsg}`);
      setLoading(false);
    }
  }, [initialPrompt, sendPrompt, exit]);

  const handleOnboardingComplete = useCallback(() => {
    setNeedsOnboarding(false);
    const client = clientRef.current;
    if (client) createSession(client);
  }, [createSession]);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        setStatus("initializing…");

        const client = new GooseClient(
          () => ({
            sessionUpdate: async (params: SessionNotification) => {
              const update = params.update;
              if (update.sessionUpdate === "agent_message_chunk") {
                if (update.content.type === "text") {
                  streamBuf.current += update.content.text;
                  appendAgent(update.content.text);
                }
              } else if (update.sessionUpdate === "tool_call") {
                handleToolCall(update);
              } else if (update.sessionUpdate === "tool_call_update") {
                handleToolCallUpdate(update);
              }
            },
            requestPermission: async (
              params: RequestPermissionRequest,
            ): Promise<RequestPermissionResponse> => {
              return new Promise<RequestPermissionResponse>((resolve) => {
                setPendingPermission({
                  toolTitle: params.toolCall.title ?? "unknown tool",
                  options: params.options.map((o) => ({
                    optionId: o.optionId,
                    name: o.name,
                    kind: o.kind,
                  })),
                  resolve,
                });
                setPermissionIdx(0);
              });
            },
          }),
          serverConnection,
        );

        if (cancelled) return;
        clientRef.current = client;

        setStatus("handshaking…");
        await client.initialize({
          protocolVersion: 0,
          clientInfo: { name: "goose-text", version: "0.1.0" },
          clientCapabilities: {},
        });
        if (cancelled) return;

        setStatus("checking provider…");
        let hasProvider = false;
        try {
          const resp = await client.goose.GooseConfigRead({ key: "GOOSE_PROVIDER" });
          hasProvider = resp.value != null && resp.value !== "" && resp.value !== "null";
        } catch {
          hasProvider = false;
        }
        if (cancelled) return;

        if (!hasProvider && !initialPrompt) {
          setNeedsOnboarding(true);
          setLoading(false);
          setStatus("setup required");
          return;
        }

        await createSession(client);
      } catch (e: unknown) {
        if (cancelled) return;
        const errorMsg = formatError(e);
        setStatus(`failed: ${errorMsg}`);
        setLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [
    serverConnection, initialPrompt, createSession,
    appendAgent, handleToolCall, handleToolCallUpdate, exit,
  ]);

  const handleSubmit = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (!trimmed) return;
      setInput("");
      setPastedFull(null);
      setViewTurnIdx(-1);
      setToolCallsExpanded(false);
      setScrollOffset(0);

      if (loading || isProcessingRef.current) {
        queueRef.current.push(trimmed);
        setQueuedMessages([...queueRef.current]);
      } else {
        sendPrompt(trimmed);
      }
    },
    [loading, sendPrompt],
  );

  useInput((ch, key) => {
    if (key.escape || (ch === "c" && key.ctrl)) {
      if (pendingPermission) { resolvePermission("cancelled"); return; }
      if (key.escape && pastedFull !== null) return;
      exit();
    }

    if (!loading && !pendingPermission && sessionIdRef.current) {
      if (key.ctrl && (ch === "p" || ch === "P")) { setOverlay({ screen: "configure", intent: "provider" }); return; }
      if (key.ctrl && (ch === "m" || ch === "M")) { setOverlay({ screen: "configure", intent: "model" }); return; }
      if (key.ctrl && (ch === "e" || ch === "E")) { setOverlay({ screen: "extensions" }); return; }
      if (ch === "g" && key.ctrl) { setOverlay({ screen: "configure", intent: "provider" }); return; }
    }

    if (pendingPermission) {
      const opts = pendingPermission.options;
      if (key.upArrow) { setPermissionIdx((i) => (i - 1 + opts.length) % opts.length); return; }
      if (key.downArrow) { setPermissionIdx((i) => (i + 1) % opts.length); return; }
      if (key.return) {
        const sel = opts[permissionIdx];
        if (sel) resolvePermission({ optionId: sel.optionId });
        return;
      }
      const keyMap: Record<string, string> = {
        y: "allow_once", a: "allow_always", n: "reject_once", N: "reject_always",
      };
      const kind = keyMap[ch];
      if (kind) {
        const m = opts.find((o) => o.kind === kind);
        if (m) resolvePermission({ optionId: m.optionId });
      }
      return;
    }

    const viewingHistory = viewTurnIdx !== -1 && viewTurnIdx < turns.length - 1;
    const multilineOwnsArrows =
      !pendingPermission && !initialPrompt && !viewingHistory && pastedFull === null;

    if (key.tab) {
      const idx = viewTurnIdx === -1 ? turns.length - 1 : viewTurnIdx;
      const t = turns[idx];
      if (t && t.responseItems.some((it) => it.itemType === "tool_call")) {
        setToolCallsExpanded((prev) => !prev);
      }
      return;
    }

    if (key.upArrow && !key.shift) {
      if (!multilineOwnsArrows) setScrollOffset((prev) => prev + 3);
      return;
    }
    if (key.downArrow && !key.shift) {
      if (!multilineOwnsArrows) setScrollOffset((prev) => Math.max(prev - 3, 0));
      return;
    }

    if (key.upArrow && key.shift) {
      setTurns((cur) => {
        if (cur.length <= 1) return cur;
        setViewTurnIdx((prev) => {
          const eff = prev === -1 ? cur.length - 1 : prev;
          return Math.max(eff - 1, 0);
        });
        return cur;
      });
      return;
    }
    if (key.downArrow && key.shift) {
      setTurns((cur) => {
        if (cur.length <= 1) return cur;
        setViewTurnIdx((prev) => {
          if (prev === -1) return -1;
          const next = prev + 1;
          return next >= cur.length ? -1 : next;
        });
        return cur;
      });
      return;
    }
  }, { isActive: !needsOnboarding && !overlay });

  const PAD_X = 2;
  const PAD_Y = 1;
  const safeTermWidth = Math.max(termWidth, 40);
  const safeTermHeight = Math.max(termHeight, 10);
  const contentWidth = Math.max(safeTermWidth - PAD_X * 2, 20);

  const effectiveTurnIdx = viewTurnIdx === -1 ? turns.length - 1 : viewTurnIdx;
  const currentTurn = turns[effectiveTurnIdx];
  const isViewingHistory = viewTurnIdx !== -1 && viewTurnIdx < turns.length - 1;
  const isLatest = !isViewingHistory;
  const showInputBar = !pendingPermission && !initialPrompt && !isViewingHistory;

  const headerH = 2;
  const isPasteMode = pastedFull !== null;
  const inputContentRows = showInputBar
    ? isPasteMode
      ? 1
      : Math.min(Math.max(input.split("\n").length, 1), INPUT_MAX_ROWS)
    : 0;
  const inputExtraLines =
    (isPasteMode ? 1 : 0) + (queuedMessages.length > 0 ? 1 : 0);
  const inputBarH = showInputBar ? 2 + inputContentRows + inputExtraLines : 0;
  const historyBarH = isViewingHistory ? 2 : 0;
  const viewportHeight = Math.max(
    safeTermHeight - PAD_Y * 2 - headerH - inputBarH - historyBarH,
    3,
  );

  const contentLines = buildContentLines({
    turn: currentTurn,
    turnIndex: effectiveTurnIdx,
    width: contentWidth,
    loading: isLatest && loading,
    status,
    spinIdx,
    pendingPermission: isLatest ? pendingPermission : null,
    permissionIdx,
    toolCallsExpanded,
    queuedMessages: isLatest ? queuedMessages : [],
  });

  if (needsOnboarding && clientRef.current) {
    return (
      <Box
        flexDirection="column"
        width={safeTermWidth}
        height={safeTermHeight}
      >
        <Onboarding
          client={clientRef.current}
          width={safeTermWidth}
          height={safeTermHeight}
          onComplete={handleOnboardingComplete}
        />
      </Box>
    );
  }

  if (overlay && clientRef.current && sessionIdRef.current) {
    if (overlay.screen === "configure") {
      const intent = overlay.intent;
      return (
        <Box flexDirection="column" width={safeTermWidth} height={safeTermHeight}>
          <ConfigureScreen
            client={clientRef.current}
            sessionId={sessionIdRef.current}
            width={safeTermWidth}
            height={safeTermHeight}
            onComplete={() => { setOverlay(null); setStatus("ready"); }}
            onCancel={() => setOverlay(null)}
            initialIntent={intent}
          />
        </Box>
      );
    } else if (overlay.screen === "extensions") {
      return (
        <Box flexDirection="column" width={safeTermWidth} height={safeTermHeight}>
          <ExtensionsManager
            client={clientRef.current}
            sessionId={sessionIdRef.current}
            height={safeTermHeight}
            onClose={() => setOverlay(null)}
          />
        </Box>
      );
    }
  }

  return (
    <Box
      flexDirection="column"
      width={safeTermWidth}
      height={safeTermHeight}
      paddingX={PAD_X}
      paddingY={PAD_Y}
    >
      {bannerVisible ? (
        <SplashScreen
          animFrame={gooseFrame}
          width={contentWidth}
          height={Math.max(safeTermHeight - PAD_Y * 2 - inputBarH, 0)}
          status={status}
          loading={loading}
          spinIdx={spinIdx}
        />
      ) : (
        <>
          <Header
            width={contentWidth}
            status={status}
            loading={loading}
            spinIdx={spinIdx}
            hasPendingPermission={!!pendingPermission}
            turnInfo={
              turns.length > 1
                ? { current: effectiveTurnIdx + 1, total: turns.length }
                : undefined
            }
          />

          <Viewport
            lines={contentLines}
            height={viewportHeight}
            width={contentWidth}
            scrollOffset={scrollOffset}
          />

          {isViewingHistory && (
            <Box flexDirection="column" width={contentWidth} flexShrink={0}>
              <Rule width={contentWidth} />
              <Box justifyContent="center" width={contentWidth}>
                <Text color={GOLD}>
                  turn {effectiveTurnIdx + 1}/{turns.length}
                </Text>
                <Text color={TEXT_DIM}> — shift+↓ to return</Text>
              </Box>
            </Box>
          )}
        </>
      )}
      {showInputBar && (
        <InputBar
          width={contentWidth}
          input={input}
          onChange={setInput}
          onSubmit={handleSubmit}
          queued={queuedMessages.length > 0}
          scrollHint={!bannerVisible && turns.length > 1}
          placeholder={bannerVisible ? INITIAL_GREETING : undefined}
          focused={showInputBar}
          pastedFull={pastedFull}
          onPastedFullChange={setPastedFull}
        />
      )}
    </Box>
  );
}

const cli = meow(
  `
  Usage
    $ goose

  Options
    --server, -s  Server URL (default: auto-launch bundled server)
    --text, -t    Send a single prompt and exit
`,
  {
    importMeta: import.meta,
    flags: {
      server: { type: "string", shortFlag: "s" },
      text: { type: "string", shortFlag: "t" },
    },
  },
);



let serverProcess: ReturnType<typeof spawn> | null = null;

async function runTextMode(serverConnection: Stream | string, prompt: string) {
  try {
    const client = new GooseClient(
      () => ({
        sessionUpdate: async (params: SessionNotification) => {
          const update = params.update;
          if (update.sessionUpdate === "agent_message_chunk") {
            if (update.content.type === "text") {
              process.stdout.write(update.content.text);
            }
          }
        },
        requestPermission: async (
          params: RequestPermissionRequest,
        ): Promise<RequestPermissionResponse> => {
          // Auto-reject in text mode
          const rejectOption = params.options.find(o => o.kind === "reject_once");
          if (rejectOption) {
            return {
              outcome: { outcome: "selected", optionId: rejectOption.optionId },
            };
          }
          return { outcome: { outcome: "cancelled" } };
        },
      }),
      serverConnection,
    );

    await client.initialize({
      protocolVersion: 0,
      clientInfo: { name: "goose-text", version: "0.1.0" },
      clientCapabilities: {},
    });

    const session = await client.newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });

    await client.prompt({
      sessionId: session.sessionId,
      prompt: [{ type: "text", text: prompt }],
    });

    process.stdout.write("\n");
  } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
    console.error(`Error: ${errMsg}`);
    process.exit(1);
  }
}

async function main() {
  let serverConnection: Stream | string;

  if (cli.flags.server) {
    serverConnection = cli.flags.server;
  } else {
    const binary = (() => {
      const __dirname = dirname(fileURLToPath(import.meta.url));
      const candidates = [
        join(__dirname, "..", "server-binary.json"),
        join(__dirname, "server-binary.json"),
      ];
      for (const candidate of candidates) {
        try {
          const data = JSON.parse(readFileSync(candidate, "utf-8"));
          return data.binaryPath ?? null;
        } catch {
          // not found here, try next
        }
      }
      return null;
    })();
    if (!binary) {
      console.error(
        "No goose binary found. Use --server <url> or install the native package.",
      );
      process.exit(1);
    }

    serverProcess = spawn(binary, ["acp"], {
      stdio: ["pipe", "pipe", "ignore"],
      detached: false,
    });

    serverProcess.on("error", (err) => {
      console.error(`Failed to start goose acp: ${err.message}`);
      process.exit(1);
    });

    const output = Writable.toWeb(serverProcess.stdin!) as WritableStream<Uint8Array>;
    const input = Readable.toWeb(serverProcess.stdout!) as ReadableStream<Uint8Array>;
    serverConnection = ndJsonStream(output, input);
  }

  // Text mode: bypass TUI and stream directly to stdout
  if (cli.flags.text) {
    await runTextMode(serverConnection, cli.flags.text);
    cleanup();
    return;
  }

  // Interactive TUI mode
  const { waitUntilExit } = render(
    <App serverConnection={serverConnection} initialPrompt={cli.flags.text} />,
  );

  await waitUntilExit();
  cleanup();
}

function cleanup() {
  if (serverProcess && !serverProcess.killed) {
    serverProcess.kill();
  }
}

process.on("exit", cleanup);
process.on("SIGINT", () => { cleanup(); process.exit(0); });
process.on("SIGTERM", () => { cleanup(); process.exit(0); });

main().catch((err) => {
  console.error(err);
  cleanup();
  process.exit(1);
});
