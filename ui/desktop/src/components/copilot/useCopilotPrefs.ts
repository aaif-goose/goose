import { useCallback, useEffect, useRef, useState } from 'react';
import { getPrefs, putPrefs } from '../../api/sdk.gen';
import type { CopilotPrefs } from '../../api/types.gen';

const CACHE_KEY = 'goose-copilot:prefs-cache';
const LEGACY_PREFS_KEY = 'goose-copilot:preferences';
const LEGACY_INSTRUCTIONS_KEY = 'goose-copilot:custom-instructions';
const SYNC_DEBOUNCE_MS = 500;

export type SyncState =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'syncing' }
  | { kind: 'synced'; switchboardSynced: boolean }
  | { kind: 'failed'; error: string };

export interface UseCopilotPrefs {
  prefs: CopilotPrefs | null;
  /** Apply a patch optimistically and queue a sync. */
  update: (patch: Partial<CopilotPrefs>) => void;
  /** Force-flush the pending sync now (e.g. user pressed Retry). */
  retry: () => void;
  syncState: SyncState;
}

function loadCachedPrefs(): CopilotPrefs | null {
  const raw = localStorage.getItem(CACHE_KEY);
  if (raw) {
    try {
      return JSON.parse(raw) as CopilotPrefs;
    } catch {
      /* fall through */
    }
  }
  return legacyPrefsFromLocalStorage();
}

function legacyPrefsFromLocalStorage(): CopilotPrefs | null {
  const prefsRaw = localStorage.getItem(LEGACY_PREFS_KEY);
  const instructions = localStorage.getItem(LEGACY_INSTRUCTIONS_KEY);
  if (!prefsRaw && !instructions) return null;
  let parsed: Record<string, unknown> = {};
  if (prefsRaw) {
    try {
      parsed = JSON.parse(prefsRaw) as Record<string, unknown>;
    } catch {
      /* ignore */
    }
  }
  return {
    schema_version: 1,
    auto_review_on_pr_open:
      typeof parsed.autoReviewOnPrOpen === 'boolean' ? parsed.autoReviewOnPrOpen : true,
    trigger_preference: (parsed.triggerPreference as CopilotPrefs['trigger_preference']) ?? 'pr-open',
    trigger_permission: (parsed.triggerPermission as CopilotPrefs['trigger_permission']) ?? 'anyone',
    allow_act_on_issues:
      typeof parsed.allowActOnIssues === 'boolean' ? parsed.allowActOnIssues : false,
    allow_commit_on_fix:
      typeof parsed.allowCommitOnFix === 'boolean' ? parsed.allowCommitOnFix : false,
    allow_open_new_prs:
      typeof parsed.allowOpenNewPrs === 'boolean' ? parsed.allowOpenNewPrs : false,
    exhaustive_review:
      typeof parsed.exhaustiveReview === 'boolean' ? parsed.exhaustiveReview : false,
    custom_instructions: instructions ?? '',
    review_output_style:
      (parsed.reviewOutputStyle as CopilotPrefs['review_output_style']) ?? 'both',
    review_model_choice:
      (parsed.reviewModelChoice as CopilotPrefs['review_model_choice']) ?? 'default',
  };
}

function clearLegacyKeys() {
  localStorage.removeItem(LEGACY_PREFS_KEY);
  localStorage.removeItem(LEGACY_INSTRUCTIONS_KEY);
}

function cachePrefs(prefs: CopilotPrefs) {
  localStorage.setItem(CACHE_KEY, JSON.stringify(prefs));
}

export function useCopilotPrefs(): UseCopilotPrefs {
  const [prefs, setPrefs] = useState<CopilotPrefs | null>(() => loadCachedPrefs());
  const [syncState, setSyncState] = useState<SyncState>({ kind: 'loading' });
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<CopilotPrefs | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { data, error } = await getPrefs();
        if (cancelled) return;
        if (error || !data) throw new Error('failed to load prefs');
        cachePrefs(data);
        setPrefs(data);
        setSyncState({ kind: 'idle' });
      } catch (e) {
        if (cancelled) return;
        // Keep whatever localStorage gave us; mark sync as failed so the
        // user knows we couldn't refresh from the server.
        const msg = e instanceof Error ? e.message : String(e);
        setSyncState({ kind: 'failed', error: msg });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const flush = useCallback(async () => {
    const target = pendingRef.current;
    if (!target) return;
    setSyncState({ kind: 'syncing' });
    try {
      const { data, error } = await putPrefs({ body: { prefs: target } });
      if (error || !data) {
        throw new Error('PUT /copilot/prefs failed');
      }
      // Server may normalize the payload; trust its echo.
      cachePrefs(data.prefs);
      setPrefs(data.prefs);
      setSyncState({
        kind: 'synced',
        switchboardSynced: data.switchboard_synced ?? false,
      });
      clearLegacyKeys();
      pendingRef.current = null;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSyncState({ kind: 'failed', error: msg });
    }
  }, []);

  const update = useCallback(
    (patch: Partial<CopilotPrefs>) => {
      setPrefs((prev) => {
        if (!prev) return prev;
        const next = { ...prev, ...patch };
        cachePrefs(next);
        pendingRef.current = next;
        return next;
      });
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void flush();
      }, SYNC_DEBOUNCE_MS);
    },
    [flush]
  );

  const retry = useCallback(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    if (pendingRef.current) {
      void flush();
    } else if (prefs) {
      pendingRef.current = prefs;
      void flush();
    }
  }, [flush, prefs]);

  // Flush on unmount so quick toggles + close don't lose the last update.
  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        if (pendingRef.current) {
          // Fire-and-forget; component is unmounting either way.
          void flush();
        }
      }
    };
  }, [flush]);

  return { prefs, update, retry, syncState };
}
