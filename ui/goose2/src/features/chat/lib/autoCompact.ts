export const AUTO_COMPACT_THRESHOLD_CONFIG_KEY = "GOOSE_AUTO_COMPACT_THRESHOLD";
export const AUTO_COMPACT_PREFERENCES_EVENT = "goose:auto-compact-preferences";
export const DEFAULT_AUTO_COMPACT_THRESHOLD = 0.8;
export const MIN_AUTO_COMPACT_THRESHOLD_PERCENT = 1;
export const MAX_AUTO_COMPACT_THRESHOLD_PERCENT = 100;
const CONTEXT_COMPACTION_PROVIDER_IDS = new Set(["goose"]);

function coerceAutoCompactThreshold(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }

  if (typeof value === "string") {
    const parsed = Number.parseFloat(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }

  return null;
}

export function normalizeAutoCompactThreshold(value: unknown): number {
  const parsed = coerceAutoCompactThreshold(value);
  if (parsed === null) {
    return DEFAULT_AUTO_COMPACT_THRESHOLD;
  }

  if (parsed <= 0 || parsed >= 1) {
    return 1;
  }

  return parsed;
}

export function clampAutoCompactThresholdPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return Math.round(DEFAULT_AUTO_COMPACT_THRESHOLD * 100);
  }

  return Math.max(
    MIN_AUTO_COMPACT_THRESHOLD_PERCENT,
    Math.min(MAX_AUTO_COMPACT_THRESHOLD_PERCENT, Math.round(value)),
  );
}

export function autoCompactThresholdToPercent(value: unknown): number {
  const parsed = coerceAutoCompactThreshold(value);
  if (parsed === null) {
    return Math.round(DEFAULT_AUTO_COMPACT_THRESHOLD * 100);
  }

  if (parsed <= 0 || parsed >= 1) {
    return MAX_AUTO_COMPACT_THRESHOLD_PERCENT;
  }

  return clampAutoCompactThresholdPercent(parsed * 100);
}

export function autoCompactPercentToThreshold(value: number): number {
  return clampAutoCompactThresholdPercent(value) / 100;
}

function supportsContextCompactionProvider(
  providerId: string | null | undefined,
): boolean {
  return providerId != null && CONTEXT_COMPACTION_PROVIDER_IDS.has(providerId);
}

export function supportsContextCompactionControls(
  providerId: string | null | undefined,
): boolean {
  return supportsContextCompactionProvider(providerId);
}
