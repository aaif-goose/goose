import { useCallback, useEffect, useRef, useState } from "react";
import { getClient } from "@/shared/api/acpConnection";
import {
  AUTO_COMPACT_PREFERENCES_EVENT,
  DEFAULT_AUTO_COMPACT_THRESHOLD,
  normalizeAutoCompactThreshold,
} from "../lib/autoCompact";

const AUTO_COMPACT_INITIAL_RETRY_DELAY_MS = 1000;
const AUTO_COMPACT_MAX_RETRY_DELAY_MS = 30000;
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
  const retryDelayMsRef = useRef(AUTO_COMPACT_INITIAL_RETRY_DELAY_MS);

  const syncFromConfig = useCallback(async () => {
    const result = await readAutoCompactThreshold();
    return result;
  }, []);

  useEffect(() => {
    let cancelled = false;
    let retryTimer: number | null = null;

    const clearRetryTimer = () => {
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
    };

    const applyConfig = async () => {
      clearRetryTimer();
      const result = await syncFromConfig();
      if (cancelled) {
        return;
      }

      if (result.ok) {
        setAutoCompactThresholdState(
          normalizeAutoCompactThreshold(result.value),
        );
        setIsHydrated(true);
        retryDelayMsRef.current = AUTO_COMPACT_INITIAL_RETRY_DELAY_MS;
      } else {
        const delayMs = retryDelayMsRef.current;
        retryDelayMsRef.current = Math.min(
          delayMs * 2,
          AUTO_COMPACT_MAX_RETRY_DELAY_MS,
        );
        retryTimer = window.setTimeout(() => {
          void applyConfig();
        }, delayMs);
      }
    };

    const handler = () => {
      retryDelayMsRef.current = AUTO_COMPACT_INITIAL_RETRY_DELAY_MS;
      void applyConfig();
    };

    window.addEventListener(
      AUTO_COMPACT_PREFERENCES_EVENT,
      handler as EventListener,
    );
    void applyConfig();

    return () => {
      cancelled = true;
      clearRetryTimer();
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
