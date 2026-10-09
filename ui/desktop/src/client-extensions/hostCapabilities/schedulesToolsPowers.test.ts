import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHostSession } from './session';
import type { HostPermission } from './permissions';

const acpMocks = vi.hoisted(() => ({
  acpListSchedules: vi.fn(),
  acpRunScheduleNow: vi.fn(),
  acpPauseSchedule: vi.fn(),
  acpUnpauseSchedule: vi.fn(),
  listTools: vi.fn(),
  callTool: vi.fn(),
}));

vi.mock('../../acp/schedules', () => ({
  acpListSchedules: acpMocks.acpListSchedules,
  acpRunScheduleNow: acpMocks.acpRunScheduleNow,
  acpPauseSchedule: acpMocks.acpPauseSchedule,
  acpUnpauseSchedule: acpMocks.acpUnpauseSchedule,
}));

vi.mock('../../acp/permissions', () => ({
  listTools: acpMocks.listTools,
  callTool: acpMocks.callTool,
}));

const actions = {
  startChat: vi.fn(),
  createSession: vi.fn(),
  openSession: vi.fn(),
  openPage: vi.fn(),
};

async function invoke(
  permissions: HostPermission[],
  capability: string,
  method: string,
  payload?: unknown
) {
  const post = vi.fn();
  await createHostSession('demo', permissions, post, actions).handleInvoke({
    type: 'grc/host/invoke',
    capability,
    method,
    payload,
  });
  return post.mock.calls[0][0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('schedules power', () => {
  it('projects the scheduled jobs', async () => {
    acpMocks.acpListSchedules.mockResolvedValue([
      {
        id: 'daily-review',
        source: '/home/me/review.yaml',
        cron: '0 9 * * *',
        paused: false,
        currentlyRunning: true,
        lastRun: '2026-09-27T09:00:00Z',
        currentSessionId: 's1',
        jobStartTime: '2026-09-28T09:00:00Z',
      },
      { id: 'idle', source: 'x', cron: '* * * * *', paused: true, currentlyRunning: false },
    ]);

    const result = await invoke(['schedules:read'], 'schedules', 'list');

    expect(result.payload).toEqual([
      {
        id: 'daily-review',
        source: '/home/me/review.yaml',
        cron: '0 9 * * *',
        paused: false,
        currentlyRunning: true,
        lastRun: '2026-09-27T09:00:00Z',
        currentSessionId: 's1',
        startedAt: '2026-09-28T09:00:00Z',
      },
      {
        id: 'idle',
        source: 'x',
        cron: '* * * * *',
        paused: true,
        currentlyRunning: false,
        lastRun: null,
        currentSessionId: null,
        startedAt: null,
      },
    ]);
  });

  it('runs, pauses and resumes a schedule with schedules:manage', async () => {
    acpMocks.acpRunScheduleNow.mockResolvedValue({ status: 'completed', sessionId: 's9' });
    const grant: HostPermission[] = ['schedules:manage'];

    const run = await invoke(grant, 'schedules', 'runNow', { scheduleId: 'daily-review' });
    const pause = await invoke(grant, 'schedules', 'pause', { scheduleId: 'daily-review' });
    const unpause = await invoke(grant, 'schedules', 'unpause', { scheduleId: 'daily-review' });

    expect(run.payload).toEqual({ status: 'completed', sessionId: 's9' });
    expect(pause.payload).toEqual({ scheduleId: 'daily-review', paused: true });
    expect(unpause.payload).toEqual({ scheduleId: 'daily-review', paused: false });
    expect(acpMocks.acpRunScheduleNow).toHaveBeenCalledWith('daily-review');
    expect(acpMocks.acpPauseSchedule).toHaveBeenCalledWith('daily-review');
    expect(acpMocks.acpUnpauseSchedule).toHaveBeenCalledWith('daily-review');
  });

  it('keeps reading and managing separate and validates the id', async () => {
    const readOnly = await invoke(['schedules:read'], 'schedules', 'pause', { scheduleId: 'a' });
    const manageOnly = await invoke(['schedules:manage'], 'schedules', 'list');
    const invalid = await invoke(['schedules:manage'], 'schedules', 'runNow', {});

    expect(readOnly.error).toContain('schedules:manage');
    expect(manageOnly.error).toContain('schedules:read');
    expect(invalid.error).toContain('Invalid "scheduleId"');
    expect(acpMocks.acpPauseSchedule).not.toHaveBeenCalled();
    expect(acpMocks.acpRunScheduleNow).not.toHaveBeenCalled();
  });
});

describe('tools power', () => {
  const tools = [
    {
      name: 'github__list_prs',
      description: 'List PRs',
      permission: 'always_allow',
      inputSchema: {},
    },
    {
      name: 'developer__shell',
      description: 'Run shell',
      permission: 'ask_before',
      inputSchema: {},
    },
    { name: 'developer__rm', description: 'Remove', permission: 'never_allow', inputSchema: {} },
    { name: 'fs__read', description: 'Read', permission: null, inputSchema: {} },
  ];

  it('lists the tools of a session with their permission level', async () => {
    acpMocks.listTools.mockResolvedValue(tools);

    const result = await invoke(['tools:read'], 'tools', 'list', {
      sessionId: 's1',
      extensionName: 'github',
    });

    expect(acpMocks.listTools).toHaveBeenCalledWith('s1', 'github');
    expect(result.payload).toHaveLength(4);
    expect(result.payload[3]).toEqual({
      name: 'fs__read',
      description: 'Read',
      permission: null,
      inputSchema: {},
    });
  });

  it('calls a tool that is set to Always allow', async () => {
    acpMocks.listTools.mockResolvedValue(tools);
    acpMocks.callTool.mockResolvedValue({
      content: [{ type: 'text', text: 'ok' }],
      isError: false,
    });

    const result = await invoke(['tools:call'], 'tools', 'call', {
      sessionId: 's1',
      extensionName: 'github',
      name: 'github__list_prs',
      arguments: { repo: 'aaif-goose/goose' },
    });

    expect(acpMocks.callTool).toHaveBeenCalledWith('s1', 'github', 'github__list_prs', {
      repo: 'aaif-goose/goose',
    });
    expect(result.payload).toEqual({ content: [{ type: 'text', text: 'ok' }], isError: false });
  });

  it('refuses tools that are not set to Always allow', async () => {
    acpMocks.listTools.mockResolvedValue(tools);

    for (const name of ['developer__shell', 'developer__rm', 'fs__read']) {
      const result = await invoke(['tools:call'], 'tools', 'call', {
        sessionId: 's1',
        extensionName: 'developer',
        name,
      });
      expect(result.error).toContain('Always allow');
    }
    expect(acpMocks.callTool).not.toHaveBeenCalled();
  });

  it('refuses unknown tools and invalid payloads', async () => {
    acpMocks.listTools.mockResolvedValue(tools);

    const unknown = await invoke(['tools:call'], 'tools', 'call', {
      sessionId: 's1',
      extensionName: 'github',
      name: 'github__nope',
    });
    const invalid = await invoke(['tools:call'], 'tools', 'call', { sessionId: 's1' });

    expect(unknown.error).toBe('Unknown tool "github__nope" in extension "github"');
    expect(invalid.error).toContain('Invalid "extensionName"');
    expect(acpMocks.callTool).not.toHaveBeenCalled();
  });

  it('separates listing from calling', async () => {
    const listOnly = await invoke(['tools:read'], 'tools', 'call', {
      sessionId: 's1',
      extensionName: 'github',
      name: 'github__list_prs',
    });
    const callOnly = await invoke(['tools:call'], 'tools', 'list', { sessionId: 's1' });

    expect(listOnly.error).toContain('tools:call');
    expect(callOnly.error).toContain('tools:read');
    expect(acpMocks.listTools).not.toHaveBeenCalled();
  });
});

describe('sessions.create', () => {
  it('creates a session in the given folder', async () => {
    actions.createSession.mockResolvedValue('new-session');

    const result = await invoke(['sessions:create'], 'sessions', 'create', {
      workingDir: '/work/app',
    });

    expect(actions.createSession).toHaveBeenCalledWith('/work/app');
    expect(result.payload).toEqual({ sessionId: 'new-session' });
  });

  it('falls back to the default folder and requires sessions:create', async () => {
    actions.createSession.mockResolvedValue('s2');

    await invoke(['sessions:create'], 'sessions', 'create');
    const denied = await invoke(['sessions:read'], 'sessions', 'create');

    expect(actions.createSession).toHaveBeenCalledWith(undefined);
    expect(actions.createSession).toHaveBeenCalledTimes(1);
    expect(denied.error).toContain('sessions:create');
  });
});
