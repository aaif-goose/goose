import { z } from 'zod';
import {
  acpListSchedules,
  acpPauseSchedule,
  acpRunScheduleNow,
  acpUnpauseSchedule,
} from '../../../acp/schedules';
import { parsePayload } from '../payload';
import type { HostCapabilityDefinition } from '../types';

const scheduleIdPayload = z.object({ scheduleId: z.string().trim().min(1) });

export const schedulesPower: HostCapabilityDefinition = {
  id: 'schedules',
  description: 'List scheduled recipe jobs, and run, pause or resume them.',
  methods: {
    list: {
      permission: 'schedules:read',
      handle: async () => {
        const jobs = await acpListSchedules();
        return jobs.map((job) => ({
          id: job.id,
          source: job.source,
          cron: job.cron,
          paused: job.paused,
          currentlyRunning: job.currentlyRunning,
          lastRun: job.lastRun ?? null,
          currentSessionId: job.currentSessionId ?? null,
          startedAt: job.jobStartTime ?? null,
        }));
      },
    },
    runNow: {
      permission: 'schedules:manage',
      handle: async (_context, payload) => {
        const { scheduleId } = parsePayload(scheduleIdPayload, payload);
        const result = await acpRunScheduleNow(scheduleId);
        return { status: result.status, sessionId: result.sessionId ?? null };
      },
    },
    pause: {
      permission: 'schedules:manage',
      handle: async (_context, payload) => {
        const { scheduleId } = parsePayload(scheduleIdPayload, payload);
        await acpPauseSchedule(scheduleId);
        return { scheduleId, paused: true };
      },
    },
    unpause: {
      permission: 'schedules:manage',
      handle: async (_context, payload) => {
        const { scheduleId } = parsePayload(scheduleIdPayload, payload);
        await acpUnpauseSchedule(scheduleId);
        return { scheduleId, paused: false };
      },
    },
  },
};
