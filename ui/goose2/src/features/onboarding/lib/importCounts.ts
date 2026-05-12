import type {
  OnboardingImportCandidate,
  OnboardingImportCounts,
  TFunctionLike,
} from "../types";

export const COUNT_KEYS = [
  "providers",
  "extensions",
  "sessions",
  "skills",
  "projects",
  "preferences",
] as const;

export type CountKey = (typeof COUNT_KEYS)[number];

export function emptyImportCounts(): OnboardingImportCounts {
  return {
    providers: 0,
    extensions: 0,
    sessions: 0,
    skills: 0,
    projects: 0,
    preferences: 0,
  };
}

export function hasImportableCounts(candidate: OnboardingImportCandidate) {
  return COUNT_KEYS.some((key) => candidate.counts[key] > 0);
}

export function sumImportCounts(
  imported: OnboardingImportCounts,
  skipped: OnboardingImportCounts,
) {
  const counts = emptyImportCounts();
  for (const key of COUNT_KEYS) {
    counts[key] = imported[key] + skipped[key];
  }
  return counts;
}

export function formatCount(key: CountKey, count: number, t: TFunctionLike) {
  return t(count === 1 ? `counts.${key}` : `counts.${key}_plural`, {
    count,
  });
}

export function formatCandidateCounts(
  candidate: OnboardingImportCandidate,
  t: TFunctionLike,
) {
  const parts = COUNT_KEYS.filter((key) => candidate.counts[key] > 0).map(
    (key) => formatCount(key, candidate.counts[key], t),
  );
  return parts.length > 0 ? parts.join(", ") : t("counts.none");
}
