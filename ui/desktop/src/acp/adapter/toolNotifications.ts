import type { ToolCallUpdate } from '@agentclientprotocol/sdk';
import type {
  LiveOutputNotificationChunk,
  LiveOutputNotificationParams,
  NotificationEvent,
} from '../../types/message';
import type { AcpChatStateChange } from './shared';
import { isRecord } from './shared';

type ToolNotification =
  | {
      type: 'message';
      params: LoggingMessageNotificationParams;
    }
  | {
      type: 'progress';
      params: ProgressNotificationParams;
    }
  | {
      type: 'platform_event';
      params: PlatformEventParams;
    }
  | {
      type: 'live_output';
      params: LiveOutputNotificationParams;
    };

type LoggingMessageNotificationParams = {
  level: string;
  logger?: string;
  data: unknown;
};

type ProgressNotificationParams = {
  progressToken: string | number;
  progress: number;
  total?: number;
  message?: string;
};

type PlatformEventParams = Record<string, unknown>;

function isLiveOutputChunk(value: unknown): value is LiveOutputNotificationChunk {
  return (
    isRecord(value) &&
    (value.stream === 'stdout' || value.stream === 'stderr') &&
    typeof value.output === 'string'
  );
}

export function toolNotificationChange(
  update: ToolCallUpdate
): Extract<AcpChatStateChange, { type: 'notification' }> | undefined {
  const notification = toolNotificationEvent(update);
  if (!notification) {
    return undefined;
  }

  return {
    type: 'notification',
    notification,
  };
}

export function toolNotificationEvent(update: ToolCallUpdate): NotificationEvent | undefined {
  const toolNotification = parseToolNotification(update._meta);
  if (!toolNotification) {
    return undefined;
  }

  return toNotificationEvent(update.toolCallId, toolNotification);
}

function parseToolNotification(meta: unknown): ToolNotification | undefined {
  if (!isRecord(meta)) {
    return undefined;
  }

  const toolNotification = meta.toolNotification;
  if (!isRecord(toolNotification)) {
    return undefined;
  }

  if (toolNotification.type === 'message') {
    const params = parseLoggingMessageParams(toolNotification.params);
    return params ? { type: 'message', params } : undefined;
  }

  if (toolNotification.type === 'progress') {
    const params = parseProgressParams(toolNotification.params);
    return params ? { type: 'progress', params } : undefined;
  }

  if (toolNotification.type === 'platform_event') {
    const params = parsePlatformEventParams(toolNotification.params);
    return params ? { type: 'platform_event', params } : undefined;
  }

  if (toolNotification.type === 'live_output') {
    const params = parseLiveOutputParams(toolNotification.params);
    return params ? { type: 'live_output', params } : undefined;
  }

  return undefined;
}

function parseLoggingMessageParams(value: unknown): LoggingMessageNotificationParams | undefined {
  if (!isRecord(value) || typeof value.level !== 'string' || !('data' in value)) {
    return undefined;
  }

  return {
    level: value.level,
    ...(typeof value.logger === 'string' ? { logger: value.logger } : {}),
    data: value.data,
  };
}

function parseProgressParams(value: unknown): ProgressNotificationParams | undefined {
  if (
    !isRecord(value) ||
    (typeof value.progressToken !== 'string' && typeof value.progressToken !== 'number') ||
    typeof value.progress !== 'number'
  ) {
    return undefined;
  }

  return {
    progressToken: value.progressToken,
    progress: value.progress,
    ...(typeof value.total === 'number' ? { total: value.total } : {}),
    ...(typeof value.message === 'string' ? { message: value.message } : {}),
  };
}

function parsePlatformEventParams(value: unknown): PlatformEventParams | undefined {
  return isRecord(value) ? value : undefined;
}

function parseLiveOutputParams(value: unknown): LiveOutputNotificationParams | undefined {
  if (
    !isRecord(value) ||
    typeof value.sequence !== 'number' ||
    !Array.isArray(value.chunks) ||
    !value.chunks.every(isLiveOutputChunk) ||
    typeof value.truncated !== 'boolean'
  ) {
    return undefined;
  }

  return {
    sequence: value.sequence,
    chunks: value.chunks,
    truncated: value.truncated,
  };
}

function toNotificationEvent(
  toolCallId: string,
  toolNotification: ToolNotification
): NotificationEvent {
  return {
    type: 'Notification',
    request_id: toolCallId,
    message: {
      method: notificationMethod(toolNotification),
      params: toolNotification.params,
    },
  };
}

function notificationMethod(toolNotification: ToolNotification): string {
  switch (toolNotification.type) {
    case 'message':
      return 'notifications/message';
    case 'progress':
      return 'notifications/progress';
    case 'platform_event':
      return 'platform_event';
    case 'live_output':
      return 'goose/live_output';
  }
}
