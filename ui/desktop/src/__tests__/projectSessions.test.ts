import { describe, expect, it } from 'vitest';
import {
  getProjectLabel,
  groupSessionsByProject,
  resolveNewChatWorkingDir,
} from '../utils/projectSessions';
import type { Session } from '../api';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'session-1',
    name: 'Session',
    message_count: 1,
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
    working_dir: '/tmp/goose',
    extension_data: { active: [], installed: [] },
    ...overrides,
  };
}

describe('groupSessionsByProject', () => {
  it('groups sessions with the same working directory', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', working_dir: '/tmp/goose' }),
      makeSession({ id: 'b', working_dir: '/tmp/goose' }),
      makeSession({ id: 'c', working_dir: '/tmp/other' }),
    ]);

    expect(groups).toHaveLength(2);
    expect(groups.find((group) => group.path === '/tmp/goose')?.sessions).toHaveLength(2);
    expect(groups.find((group) => group.path === '/tmp/other')?.sessions).toHaveLength(1);
  });

  it('sorts project groups by most recent session', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'old', working_dir: '/tmp/old', updated_at: '2026-01-01T00:00:00.000Z' }),
      makeSession({ id: 'new', working_dir: '/tmp/new', updated_at: '2026-01-03T00:00:00.000Z' }),
      makeSession({
        id: 'middle',
        working_dir: '/tmp/middle',
        updated_at: '2026-01-02T00:00:00.000Z',
      }),
    ]);

    expect(groups.map((group) => group.path)).toEqual(['/tmp/new', '/tmp/middle', '/tmp/old']);
  });

  it('sorts sessions within each project newest first', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'old', updated_at: '2026-01-01T00:00:00.000Z' }),
      makeSession({ id: 'new', updated_at: '2026-01-03T00:00:00.000Z' }),
      makeSession({ id: 'middle', updated_at: '2026-01-02T00:00:00.000Z' }),
    ]);

    expect(groups[0].sessions.map((session) => session.id)).toEqual(['new', 'middle', 'old']);
  });

  it('normalizes empty working directories into one group', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', working_dir: '' }),
      makeSession({ id: 'b', working_dir: '   ' }),
    ]);

    expect(groups).toHaveLength(1);
    expect(groups[0].path).toBe('');
    expect(groups[0].label).toBe('Unknown');
    expect(groups[0].sessions).toHaveLength(2);
  });

  it('returns an empty array for empty input', () => {
    expect(groupSessionsByProject([])).toEqual([]);
  });

  it('disambiguates projects with the same basename', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', working_dir: '/Users/me/work/goose' }),
      makeSession({ id: 'b', working_dir: '/Users/me/forks/goose' }),
    ]);

    expect(groups.map((group) => group.label).sort()).toEqual(['forks/goose', 'work/goose']);
  });
});

describe('getProjectLabel', () => {
  it('extracts the basename from an absolute path', () => {
    expect(getProjectLabel('/Users/me/work/goose')).toBe('goose');
  });

  it('handles the root path', () => {
    expect(getProjectLabel('/')).toBe('/');
  });

  it('handles an empty path', () => {
    expect(getProjectLabel('')).toBe('Unknown');
  });

  it('handles Windows-style paths', () => {
    expect(getProjectLabel('C:\\Users\\me\\goose')).toBe('goose');
  });
});

describe('resolveNewChatWorkingDir', () => {
  const sessions = [
    makeSession({ id: 'active', working_dir: '/tmp/active' }),
    makeSession({ id: 'other', working_dir: '/tmp/other' }),
  ];

  it('returns the active session working directory when found', () => {
    expect(resolveNewChatWorkingDir('active', sessions, '/tmp/fallback')).toBe('/tmp/active');
  });

  it('returns fallback when there is no active session id', () => {
    expect(resolveNewChatWorkingDir(undefined, sessions, '/tmp/fallback')).toBe('/tmp/fallback');
  });

  it('returns fallback when the active session is not found', () => {
    expect(resolveNewChatWorkingDir('missing', sessions, '/tmp/fallback')).toBe('/tmp/fallback');
  });
});
