import type { SessionListItem } from '../acp/sessions';

export interface ProjectGroup {
  path: string;
  label: string;
  sessions: SessionListItem[];
  lastActivityAt: string;
}

function getSessionActivityTime(session: SessionListItem): string {
  return session.lastMessageAt ?? session.updatedAt;
}

const UNKNOWN_PROJECT_LABEL = 'Unknown';

export function normalizeProjectPath(workingDir: string): string {
  const normalized = workingDir.trim();
  if (!normalized) {
    return '';
  }

  const withoutTrailingSeparators = normalized.replace(/[\\/]+$/, '');
  return withoutTrailingSeparators || normalized;
}

export function getProjectLabel(workingDir: string): string {
  const normalized = workingDir.trim();
  if (!normalized) {
    return UNKNOWN_PROJECT_LABEL;
  }

  const withoutTrailingSeparators = normalizeProjectPath(workingDir);
  if (!withoutTrailingSeparators) {
    return normalized;
  }

  const parts = withoutTrailingSeparators.split(/[\\/]+/);
  return parts[parts.length - 1] || normalized;
}

export function groupSessionsByProject(sessions: SessionListItem[]): ProjectGroup[] {
  const groups = new Map<string, SessionListItem[]>();

  for (const session of sessions) {
    const path = normalizeProjectPath(session.workingDir);
    const existing = groups.get(path);
    if (existing) {
      existing.push(session);
    } else {
      groups.set(path, [session]);
    }
  }

  const baseGroups = Array.from(groups.entries()).map(([path, projectSessions]) => {
    const sortedSessions = [...projectSessions].sort(
      (a, b) =>
        new Date(getSessionActivityTime(b)).getTime() -
        new Date(getSessionActivityTime(a)).getTime()
    );
    return {
      path,
      label: getProjectLabel(path),
      sessions: sortedSessions,
      lastActivityAt: getSessionActivityTime(sortedSessions[0] ?? { updatedAt: '' } as SessionListItem),
    };
  });

  const labelCounts = baseGroups.reduce((counts, group) => {
    counts.set(group.label, (counts.get(group.label) ?? 0) + 1);
    return counts;
  }, new Map<string, number>());

  return baseGroups
    .map((group) => ({
      ...group,
      label:
        (labelCounts.get(group.label) ?? 0) > 1
          ? getDisambiguatedProjectLabel(group.path)
          : group.label,
    }))
    .sort(
      (a, b) => new Date(b.lastActivityAt).getTime() - new Date(a.lastActivityAt).getTime()
    );
}

function getDisambiguatedProjectLabel(workingDir: string): string {
  const withoutTrailingSeparators = normalizeProjectPath(workingDir);
  if (!withoutTrailingSeparators) {
    return UNKNOWN_PROJECT_LABEL;
  }
  const parts = withoutTrailingSeparators.split(/[\\/]+/).filter(Boolean);
  if (parts.length >= 2) {
    return `${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
  }

  return getProjectLabel(workingDir);
}
