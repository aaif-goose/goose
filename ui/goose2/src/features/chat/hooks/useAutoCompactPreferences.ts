import { useCallback, useEffect, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  DEFAULT_AUTO_COMPACT_THRESHOLD,
  normalizeAutoCompactThreshold,
} from "../lib/autoCompact";

const AUTO_COMPACT_RETRY_DELAY_MS = 1000;
const AUTO_COMPACT_THRESHOLD_PREFERENCE_KEY = "autoCompactThreshold";

type ConfigReadResult =
  | {
      ok: true;
      value: unknown;
    }
  | {
      ok: false;
    };

async function readAutoCompactThreshold(): Promise<ConfigReadResult> {
  try {
    const client = await getClient();
    const response = await client.goose.GoosePreferencesRead({
      keys: [AUTO_COMPACT_THRESHOLD_PREFERENCE_KEY],
    });
    const preference = response.values.find(
      (value) => value.key === AUTO_COMPACT_THRESHOLD_PREFERENCE_KEY,
    );
    return {
      ok: true,
      value: preference?.value ?? null,
    };
  } catch {
    return { ok: false };
  }
}

async function writeAutoCompactThreshold(value: number): Promise<void> {
  const client = await getClient();
  await client.goose.GoosePreferencesSave({
    values: [{ key: AUTO_COMPACT_THRESHOLD_PREFERENCE_KEY, value }],
  });
}

export function useAutoCompactPreferences() {
  const [autoCompactThreshold, setAutoCompactThresholdState] = useState(
    DEFAULT_AUTO_COMPACT_THRESHOLD,
  );
  const [isHydrated, setIsHydrated] = useState(false);
  const [syncVersion, setSyncVersion] = useState(0);

  const requestSyncFromConfig = useCallback(() => {
    setSyncVersion((current) => current + 1);
  }, []);

  useEffect(() => {
    const handler = () => {
      requestSyncFromConfig();
    };
    window.addEventListener(
      AUTO_COMPACT_PREFERENCES_EVENT,
      handler as EventListener,
    );
    return () => {
      window.removeEventListener(
        AUTO_COMPACT_PREFERENCES_EVENT,
        handler as EventListener,
      );
    };
  }, [requestSyncFromConfig]);

  const syncFromConfig = useCallback(async (_syncVersion: number) => {
    void _syncVersion;
    const result = await readAutoCompactThreshold();
    return result;
  }, []);

  useEffect(() => {
    let cancelled = false;
    let retryTimer: number | null = null;

    const applyConfig = async () => {
      const result = await syncFromConfig(syncVersion);
      if (cancelled) {
        return;
      }

      if (result.ok) {
        setAutoCompactThresholdState(
          normalizeAutoCompactThreshold(result.value),
        );
      } else {
        retryTimer = window.setTimeout(
          requestSyncFromConfig,
          AUTO_COMPACT_RETRY_DELAY_MS,
        );
      }
      setIsHydrated(true);
    };

    void applyConfig();

    return () => {
      cancelled = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [requestSyncFromConfig, syncFromConfig, syncVersion]);

  const dispatchPreferencesEvent = useCallback(() => {
    window.dispatchEvent(new Event(AUTO_COMPACT_PREFERENCES_EVENT));
  }, []);

  const setAutoCompactThreshold = useCallback(
    async (value: number) => {
      const normalized = normalizeAutoCompactThreshold(value);
      await writeAutoCompactThreshold(normalized);
      setAutoCompactThresholdState(normalized);
      setIsHydrated(true);
      dispatchPreferencesEvent();
    },
    [dispatchPreferencesEvent],
  );

  return {
    autoCompactThreshold,
    isHydrated,
    setAutoCompactThreshold,
  };
}
