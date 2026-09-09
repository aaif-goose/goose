// Pure formatting helpers for per-message usage stats. Unit labels are left to
// the consuming component so they can be localized.

/** `842`, `1.2k`, `12k`, `1.2M` — one decimal below 10k/10M, trailing `.0` stripped. */
export function formatTokenCount(n: number): string {
  if (n < 1000) {
    return Math.round(n).toString();
  }
  const [value, suffix] = n >= 1_000_000 ? [n / 1_000_000, 'M'] : [n / 1000, 'k'];
  const text = value < 10 ? value.toFixed(1).replace(/\.0$/, '') : Math.round(value).toString();
  return `${text}${suffix}`;
}

/** Adaptive precision so small per-message costs stay meaningful: `$1.24`, `$0.012`, `$0.0004`. */
export function formatCost(cost: number): string {
  if (cost === 0) {
    return '$0.00';
  }
  const decimals = cost >= 1 ? 2 : cost >= 0.01 ? 3 : 4;
  return `$${cost.toFixed(decimals)}`;
}

/** `840ms` below 1s, `4.2s` below 1m, else `1m 5s`. */
export function formatDuration(ms: number): string {
  if (ms < 1000) {
    return `${Math.round(ms)}ms`;
  }
  if (ms < 60_000) {
    return `${(ms / 1000).toFixed(1).replace(/\.0$/, '')}s`;
  }
  const totalSeconds = Math.round(ms / 1000);
  return `${Math.floor(totalSeconds / 60)}m ${totalSeconds % 60}s`;
}

/** Tokens per second, or null when either input is missing or non-positive (no NaN/Infinity). */
export function tokensPerSecond(
  outputTokens: number | null | undefined,
  elapsedMs: number | null | undefined
): number | null {
  if (!outputTokens || !elapsedMs || outputTokens <= 0 || elapsedMs <= 0) {
    return null;
  }
  return outputTokens / (elapsedMs / 1000);
}

/** Precision scaled to magnitude: `128`, `48.2`, `3.75`. */
export function formatTokensPerSecond(tps: number): string {
  if (tps >= 100) {
    return Math.round(tps).toString();
  }
  if (tps >= 10) {
    return tps.toFixed(1);
  }
  return tps.toFixed(2);
}
