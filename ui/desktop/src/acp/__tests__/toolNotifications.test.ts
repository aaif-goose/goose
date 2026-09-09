import type { ToolCallUpdate } from '@agentclientprotocol/sdk';
import { describe, expect, it } from 'vitest';
import { toolNotificationEvent } from '../adapter/toolNotifications';

function liveOutputUpdate(params: unknown): ToolCallUpdate {
  return {
    toolCallId: 'tool-1',
    status: 'in_progress',
    _meta: {
      toolNotification: {
        type: 'live_output',
        params,
      },
    },
  };
}

describe('toolNotificationEvent', () => {
  it('maps live output metadata to a tool-correlated notification', () => {
    const event = toolNotificationEvent(
      liveOutputUpdate({
        sequence: 2,
        chunks: [
          {
            stream: 'stdout',
            output: 'ready\n',
          },
          {
            stream: 'stderr',
            output: 'warning\n',
          },
        ],
        truncated: false,
      })
    );

    expect(event).toEqual({
      type: 'Notification',
      request_id: 'tool-1',
      message: {
        method: 'goose/live_output',
        params: {
          sequence: 2,
          chunks: [
            {
              stream: 'stdout',
              output: 'ready\n',
            },
            {
              stream: 'stderr',
              output: 'warning\n',
            },
          ],
          truncated: false,
        },
      },
    });
  });

  it('ignores malformed live output metadata', () => {
    expect(
      toolNotificationEvent(
        liveOutputUpdate({
          sequence: 'two',
          chunks: [],
          truncated: false,
        })
      )
    ).toBeUndefined();
  });
});
