import { exportSession } from "./acpApi";

const SNIPPET_PREFIX = 40;
const SNIPPET_SUFFIX = 60;

type MessageRole = "user" | "assistant" | "system";

const SEARCHABLE_ROLES = new Set<MessageRole>(["user", "assistant", "system"]);

const SEARCHABLE_BLOCK_TYPES = new Set([
  "text",
  "input_text",
  "output_text",
  "systemNotification",
  "system_notification",
]);

const SKIPPED_BLOCK_TYPES = new Set([
  "toolRequest",
  "toolResponse",
  "thinking",
  "redactedThinking",
  "reasoning",
  "image",
]);

export interface SessionSearchResult {
  sessionId: string;
  snippet: string;
  messageId: string;
  messageRole?: MessageRole;
  matchCount: number;
}

interface ParsedMessage {
  id: string;
  role: MessageRole | null;
  texts: string[];
}

export async function searchSessionsViaExports(
  query: string,
  sessionIds: string[],
): Promise<SessionSearchResult[]> {
  const trimmed = query.trim();
  if (!trimmed) return [];

  const unique = [...new Set(sessionIds)];
  const results: SessionSearchResult[] = [];

  for (const sessionId of unique) {
    try {
      const exported = await exportSession(sessionId);
      const result = searchSession(sessionId, exported, trimmed);
      if (result) results.push(result);
    } catch {
      // skip sessions that fail to export
    }
  }

  return results;
}

function searchSession(
  sessionId: string,
  json: string,
  query: string,
): SessionSearchResult | null {
  const root = safeParse(json);
  if (!root) return null;

  const conversation = root.conversation ?? root.messages;
  if (!conversation) return null;

  const messages = flattenMessages(conversation);
  if (!messages.length) return null;

  let firstMatch: {
    messageId: string;
    role: MessageRole | null;
    snippet: string;
  } | null = null;
  let matchCount = 0;

  for (const msg of messages) {
    for (const text of msg.texts) {
      const count = countMatches(text, query);
      if (!count) continue;
      matchCount += count;
      firstMatch ??= {
        messageId: msg.id,
        role: msg.role,
        snippet: buildSnippet(text, query),
      };
    }
  }

  if (!firstMatch) return null;

  return {
    sessionId,
    snippet: firstMatch.snippet,
    messageId: firstMatch.messageId,
    messageRole: firstMatch.role ?? undefined,
    matchCount,
  };
}

function safeParse(json: string): Record<string, unknown> | null {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

function flattenMessages(value: unknown): ParsedMessage[] {
  if (Array.isArray(value)) return value.flatMap(flattenMessages);
  if (!isObject(value)) return [];

  if ("message" in value) return flattenMessages(value.message);
  if ("messages" in value) return flattenMessages(value.messages);

  const msg = tryParseMessage(value);
  return msg ? [msg] : [];
}

function tryParseMessage(obj: Record<string, unknown>): ParsedMessage | null {
  if (!("role" in obj) || !("content" in obj || "text" in obj)) return null;

  const role = toRole(obj.role);
  const texts =
    obj.content !== undefined
      ? getSearchableTexts(obj.content, role)
      : typeof obj.text === "string" && role && obj.text.trim()
        ? [obj.text.trim()]
        : [];

  if (!texts.length) return null;

  return {
    id: typeof obj.id === "string" ? obj.id : crypto.randomUUID(),
    role,
    texts,
  };
}

function getSearchableTexts(
  value: unknown,
  role: MessageRole | null,
): string[] {
  if (typeof value === "string") {
    return role && SEARCHABLE_ROLES.has(role) && value.trim()
      ? [value.trim()]
      : [];
  }
  if (Array.isArray(value)) return value.flatMap((v) => getBlockText(v, role));
  if (isObject(value)) return getBlockText(value, role);
  return [];
}

function getBlockText(value: unknown, role: MessageRole | null): string[] {
  if (!isObject(value)) return [];
  const type = value.type as string | undefined;
  const text = (value.text as string | undefined)?.trim();
  if (!text) return [];

  if (SKIPPED_BLOCK_TYPES.has(type ?? "")) return [];
  if (SEARCHABLE_BLOCK_TYPES.has(type ?? "")) return [text];
  return role && SEARCHABLE_ROLES.has(role) ? [text] : [];
}

function toRole(value: unknown): MessageRole | null {
  if (typeof value !== "string") return null;
  const r = value.trim().toLowerCase();
  return SEARCHABLE_ROLES.has(r as MessageRole) ? (r as MessageRole) : null;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function countMatches(text: string, query: string): number {
  const hay = text.toLowerCase();
  const needle = query.toLowerCase();
  if (!needle) return 0;

  let count = 0;
  let pos = hay.indexOf(needle);
  while (pos !== -1) {
    count++;
    pos = hay.indexOf(needle, pos + needle.length);
  }
  return count;
}

function buildSnippet(text: string, query: string): string {
  const idx = text.toLowerCase().indexOf(query.toLowerCase());
  const at = idx >= 0 ? idx : 0;
  const start = Math.max(0, at - SNIPPET_PREFIX);
  const end = Math.min(text.length, at + query.length + SNIPPET_SUFFIX);

  const prefix = start > 0 ? "..." : "";
  const suffix = end < text.length ? "..." : "";
  return `${prefix}${text.substring(start, end).trim()}${suffix}`;
}
