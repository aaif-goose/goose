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

import type { ToolInfo } from '../../api/types.gen';
import { getTools } from '../../api';

type ToolsList = Array<ToolInfo>;

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

  const promise = getTools({
    query: { session_id: sessionId, extension_name: extensionName || undefined },
  })
    .then((response) => response.data ?? null)
    .catch(() => {
      // Evict on failure so the next caller retries
      cache.delete(key);
      return null;
    });

  cache.set(key, promise);
  return promise;
}

export function clearToolsCache(sessionId?: string): void {
  if (!sessionId) {
    cache.clear();
    return;
  }
  for (const key of cache.keys()) {
    if (key.startsWith(`${sessionId}:`)) {
      cache.delete(key);
    }
  }
}
