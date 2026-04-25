import type { Session } from '../api';

export interface ProjectGroup {
  path: string;
  label: string;
  sessions: Session[];
  updatedAt: Date;
}

const UNKNOWN_PROJECT_LABEL = 'Unknown';

function normalizeProjectPath(workingDir: string): string {
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

export function groupSessionsByProject(sessions: Session[]): ProjectGroup[] {
  const groups = new Map<string, Session[]>();

  for (const session of sessions) {
    const path = normalizeProjectPath(session.working_dir);
    const existing = groups.get(path);
    if (existing) {
      existing.push(session);
    } else {
      groups.set(path, [session]);
    }
  }

  const baseGroups = Array.from(groups.entries()).map(([path, projectSessions]) => {
    const sortedSessions = [...projectSessions].sort(
      (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
    );
    return {
      path,
      label: getProjectLabel(path),
      sessions: sortedSessions,
      updatedAt: new Date(sortedSessions[0]?.updated_at ?? 0),
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
    .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
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

export function resolveNewChatWorkingDir(
  activeSessionId: string | undefined,
  sessions: Session[],
  fallback: string
): string {
  if (!activeSessionId) {
    return fallback;
  }

  const activeSession = sessions.find((session) => session.id === activeSessionId);
  const workingDir = activeSession?.working_dir.trim();
  return workingDir || fallback;
}
