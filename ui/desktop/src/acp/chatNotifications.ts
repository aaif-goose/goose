import type {
  GooseSessionNotification_unstable,
  ProviderDeviceCodeNotification_unstable,
} from '@aaif/goose-acp-client';
import type { SessionNotification } from '@agentclientprotocol/sdk';
import { AppEvents } from '../constants/events';
import { maybeHandlePlatformEvent } from '../utils/platform_events';
import { toolNotificationEvent } from './adapter/toolNotifications';
import { acpChatSessionActions, acpChatSessionStore } from './chatSessionStore';
import { publishLiveVoiceInteractionEnded } from './liveVoiceNotifications';
import { publishExtensionSessionEvent } from '../client-extensions/extensionSessionEvents';

export function handleAcpSessionNotification(notification: SessionNotification): Promise<void> {
  const sessionNameBeforeNotification = acpChatSessionStore.getSnapshot(notification.sessionId)
    ?.session?.name;
  const updatedName =
    notification.update.sessionUpdate === 'session_info_update'
      ? notification.update.title
      : undefined;
  acpChatSessionActions.applyAcpSessionNotification(notification);
  maybeHandleLivePlatformEvent(notification);

  if (updatedName && updatedName !== sessionNameBeforeNotification) {
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_RENAMED, {
        detail: { sessionId: notification.sessionId, newName: updatedName },
      })
    );
  }

  const { sessionId, update } = notification;
  if (update.sessionUpdate === 'agent_message_chunk') {
    publishExtensionSessionEvent({
      type: 'agent_message_chunk',
      sessionId,
      content: update.content,
    });
  } else if (update.sessionUpdate === 'tool_call_update') {
    publishExtensionSessionEvent({
      type: 'tool_call',
      sessionId,
      toolCallId: update.toolCallId,
      title: update.title,
      status: update.status,
    });
  }

  return Promise.resolve();
}

function maybeHandleLivePlatformEvent(notification: SessionNotification): void {
  const update = notification.update;
  if (
    update.sessionUpdate !== 'tool_call_update' ||
    update.status === 'completed' ||
    update.status === 'failed'
  ) {
    return;
  }

  const event = toolNotificationEvent(update);
  if (event?.message.method === 'platform_event') {
    maybeHandlePlatformEvent(event.message, notification.sessionId);
  }
}

export function handleAcpGooseSessionNotification(
  notification: GooseSessionNotification_unstable
): Promise<void> {
  if (notification.update.sessionUpdate === 'live_voice_interaction_ended') {
    publishLiveVoiceInteractionEnded({
      sessionId: notification.sessionId,
      update: notification.update,
    });
    return Promise.resolve();
  }

  acpChatSessionActions.applyAcpGooseSessionNotification(notification);

  const update = notification.update;
  if (update.sessionUpdate === 'status_message') {
    const status = update.status;
    publishExtensionSessionEvent({
      type: 'status_message',
      sessionId: notification.sessionId,
      message: status.message,
      level: status.type,
    });
  }

  return Promise.resolve();
}

export function handleAcpProviderDeviceCodeNotification(
  notification: ProviderDeviceCodeNotification_unstable
): Promise<void> {
  window.dispatchEvent(new CustomEvent('goose:device-code', { detail: notification }));
  return Promise.resolve();
}
