import { describe, expect, it } from 'vitest';
import { getProjectLabel, groupSessionsByProject } from '../utils/projectSessions';
import type { SessionListItem } from '../acp/sessions';

function makeSession(overrides: Partial<SessionListItem> = {}): SessionListItem {
  return {
    id: 'session-1',
    name: 'Session',
    messageCount: 1,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    workingDir: '/tmp/goose',
    ...overrides,
  };
}

describe('groupSessionsByProject', () => {
  it('groups sessions by normalized working directory', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', workingDir: '/tmp/goose' }),
      makeSession({ id: 'b', workingDir: '/tmp/goose/' }),
      makeSession({ id: 'c', workingDir: '  /tmp/goose//  ' }),
      makeSession({ id: 'd', workingDir: '/tmp/other' }),
    ]);

    expect(groups).toHaveLength(2);
    expect(groups.find((group) => group.path === '/tmp/goose')?.sessions.map((s) => s.id)).toEqual([
      'a',
      'b',
      'c',
    ]);
    expect(groups.find((group) => group.path === '/tmp/other')?.sessions.map((s) => s.id)).toEqual([
      'd',
    ]);
  });

  it('sorts project groups and sessions by session activity', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'old', workingDir: '/tmp/old', updatedAt: '2026-01-01T00:00:00.000Z' }),
      makeSession({
        id: 'middle-old',
        workingDir: '/tmp/middle',
        updatedAt: '2026-01-02T00:00:00.000Z',
      }),
      makeSession({
        id: 'middle-new',
        workingDir: '/tmp/middle',
        updatedAt: '2026-01-03T00:00:00.000Z',
      }),
      makeSession({
        id: 'renamed',
        workingDir: '/tmp/new',
        updatedAt: '2026-01-04T00:00:00.000Z',
        lastMessageAt: '2026-01-01T00:00:00.000Z',
      }),
      makeSession({
        id: 'active',
        workingDir: '/tmp/new',
        updatedAt: '2026-01-02T00:00:00.000Z',
        lastMessageAt: '2026-01-05T00:00:00.000Z',
      }),
    ]);

    expect(groups.map((group) => group.path)).toEqual(['/tmp/new', '/tmp/middle', '/tmp/old']);
    expect(groups[0].sessions.map((session) => session.id)).toEqual(['active', 'renamed']);
    expect(groups[1].sessions.map((session) => session.id)).toEqual(['middle-new', 'middle-old']);
  });

  it('handles missing working directories', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', workingDir: '' }),
      makeSession({ id: 'b', workingDir: '   ' }),
    ]);

    expect(groups).toHaveLength(1);
    expect(groups[0].path).toBe('');
    expect(groups[0].label).toBe('Unknown');
    expect(groups[0].sessions.map((session) => session.id)).toEqual(['a', 'b']);
  });

  it('disambiguates projects with the same basename', () => {
    const groups = groupSessionsByProject([
      makeSession({ id: 'a', workingDir: '/Users/me/work/goose' }),
      makeSession({ id: 'b', workingDir: '/Users/me/forks/goose' }),
    ]);

    expect(groups.map((group) => group.label).sort()).toEqual(['forks/goose', 'work/goose']);
  });
});

describe('getProjectLabel', () => {
  it('extracts readable labels from paths', () => {
    expect(getProjectLabel('/Users/me/work/goose')).toBe('goose');
    expect(getProjectLabel('/')).toBe('/');
    expect(getProjectLabel('')).toBe('Unknown');
    expect(getProjectLabel('C:\\Users\\me\\goose')).toBe('goose');
  });
});
