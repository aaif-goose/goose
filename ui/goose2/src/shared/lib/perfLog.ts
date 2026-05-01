/**
 * Gated performance logger for frontend timing instrumentation.
 *
 * Enabled when any of the following is true:
 *   - Running under Vite dev (`import.meta.env.DEV`)
 *   - `localStorage.getItem("goose.perf") === "1"`
 *
 * Otherwise a no-op, so perf call sites add zero runtime cost in release
 * builds for users who have not opted in.
 *
 * Messages are prefixed with `[perf:<channel>]` by callers; this helper
 * is intentionally dumb and forwards the already-formatted string.
 */
function isEnabled(): boolean {
  try {
    if (import.meta.env?.DEV) return true;
  } catch {
    // import.meta may be unavailable in some test contexts
  }
  try {
    if (
      typeof localStorage !== "undefined" &&
      localStorage.getItem("goose.perf") === "1"
    ) {
      return true;
    }
  } catch {
    // localStorage can throw in restricted contexts
  }
  return false;
}

const enabled = isEnabled();

export function perfLog(message: string): void {
  if (!enabled) return;
  // eslint-disable-next-line no-console
  console.log(message);
}
