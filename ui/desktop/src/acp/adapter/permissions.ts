import type { RequestPermissionRequest } from '@agentclientprotocol/sdk';
import type { AcpPermissionRequest } from '../permissionRequestTypes';
import {
  type AcpChatStateChange,
  type AdapterState,
  DEFAULT_VISIBLE_MESSAGE_METADATA,
  messagesChange,
  rawInputToArguments,
  toolIdentity,
} from './shared';

export function applyPermissionRequest(
  state: AdapterState,
  permissionRequest: AcpPermissionRequest
): AcpChatStateChange[] {
  const { generation, request } = permissionRequest;
  const toolCallId = request.toolCall.toolCallId;
  removePermissionRequestFromState(state, toolCallId);

  const identity = toolIdentity(request.toolCall);
  const prompt = permissionPrompt(request);

  state.messages.push({
    id: `acp_permission_${toolCallId}`,
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'actionRequired',
        data: {
          actionType: 'toolConfirmation',
          generation,
          id: toolCallId,
          toolName: identity.toolName ?? request.toolCall.title ?? toolCallId,
          arguments: rawInputToArguments(request.toolCall.rawInput),
          ...(prompt ? { prompt } : {}),
        },
      },
    ],
    metadata: { ...DEFAULT_VISIBLE_MESSAGE_METADATA },
  });

  return messagesChange(state);
}

export function cancelPermissionRequest(
  state: AdapterState,
  toolCallId: string,
  generation: string
): AcpChatStateChange[] {
  return removePermissionRequestFromState(state, toolCallId, generation)
    ? messagesChange(state)
    : [];
}

function removePermissionRequestFromState(
  state: AdapterState,
  toolCallId: string,
  generation?: string
): boolean {
  let changed = false;

  state.messages = state.messages.flatMap((message) => {
    const content = message.content.filter((content) => {
      const matches =
        content.type === 'actionRequired' &&
        content.data.actionType === 'toolConfirmation' &&
        content.data.id === toolCallId &&
        (generation === undefined || content.data.generation === generation);
      changed ||= matches;
      return !matches;
    });

    if (content.length === message.content.length) {
      return [message];
    }

    return content.length > 0 ? [{ ...message, content }] : [];
  });

  return changed;
}

function permissionPrompt(request: RequestPermissionRequest): string | undefined {
  for (const content of request.toolCall.content ?? []) {
    if (content.type === 'content' && content.content.type === 'text') {
      return content.content.text;
    }
  }

  return undefined;
}
