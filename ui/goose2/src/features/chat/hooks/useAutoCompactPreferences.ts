import { useCallback, useEffect, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  AUTO_COMPACT_THRESHOLD_CONFIG_KEY,
  DEFAULT_AUTO_COMPACT_THRESHOLD,
  normalizeAutoCompactThreshold,
} from "../lib/autoCompact";

const AUTO_COMPACT_RETRY_DELAY_MS = 1000;

type ConfigReadResult =
  | {
      ok: true;
      value: unknown;
    }
  | {
      ok: false;
    };

async function readConfigValue(key: string): Promise<ConfigReadResult> {
  try {
    const client = await getClient();
    const response = await client.goose.GooseConfigRead({ key });
    return {
      ok: true,
      value: response.value ?? null,
    };
  } catch {
    return { ok: false };
  }
}

async function writeConfigValue(key: string, value: number): Promise<void> {
  const client = await getClient();
  await client.goose.GooseConfigUpsert({ key, value });
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
    const result = await readConfigValue(AUTO_COMPACT_THRESHOLD_CONFIG_KEY);
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
      await writeConfigValue(AUTO_COMPACT_THRESHOLD_CONFIG_KEY, normalized);
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
