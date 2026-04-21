import { useCallback, useEffect, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  AUTO_COMPACT_THRESHOLD_CONFIG_KEY,
  DEFAULT_AUTO_COMPACT_THRESHOLD,
  normalizeAutoCompactThreshold,
} from "../lib/autoCompact";

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

  const syncFromConfig = useCallback(async () => {
    const result = await readConfigValue(AUTO_COMPACT_THRESHOLD_CONFIG_KEY);
    if (result.ok) {
      setAutoCompactThresholdState(normalizeAutoCompactThreshold(result.value));
    }
    setIsHydrated(true);
  }, []);

  useEffect(() => {
    void syncFromConfig();
    const handler = () => {
      void syncFromConfig();
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
  }, [syncFromConfig]);

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
