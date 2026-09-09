/**
 * Module-level cache for MCP tool definitions fetched from /agent/tools.
 *
 * Multiple McpAppRenderer instances in the same session would otherwise each
 * fetch the full tool list on mount (N+1 problem). This cache deduplicates
 * those requests per (sessionId, extensionName) and shares the result.
 *
 * The cache stores promises so that concurrent requests for the same key
 * automatically coalesce into a single network call.
 */

import { listMcpAppTools, type McpAppTool } from '../../acp/mcp-apps';

type ToolsList = Array<McpAppTool>;

const cache = new Map<string, Promise<ToolsList | null>>();

function cacheKey(sessionId: string, extensionName: string | undefined): string {
  return `${sessionId}:${extensionName ?? ''}`;
}

export function getCachedTools(
  sessionId: string,
  extensionName: string | undefined
): Promise<ToolsList | null> {
  const key = cacheKey(sessionId, extensionName);
  const existing = cache.get(key);
  if (existing) return existing;

  const promise = listMcpAppTools(sessionId, extensionName || undefined).catch(() => {
    // Evict on failure so the next caller retries
    cache.delete(key);
    return null;
  });

  cache.set(key, promise);
  return promise;
}
