import { ActionRequired } from '../api';
import { defineMessages, useIntl } from '../i18n';
import { snakeToTitleCase } from '../utils';
import ToolApprovalButtons from './ToolApprovalButtons';

const i18n = defineMessages({
  allowToolCallWithName: {
    id: 'toolConfirmation.allowToolCallWithName',
    defaultMessage: 'Allow {toolName}?',
  },
  gooseWouldLikeToCallWithName: {
    id: 'toolConfirmation.gooseWouldLikeToCallWithName',
    defaultMessage: 'Goose would like to call {toolName}. Allow?',
  },
});

function formatToolName(fullName: string): string {
  const delimiterIndex = fullName.lastIndexOf('__');
  const shortName = delimiterIndex === -1 ? fullName : fullName.substring(delimiterIndex + 2);
  return snakeToTitleCase(shortName);
}

type ToolConfirmationData = Extract<ActionRequired['data'], { actionType: 'toolConfirmation' }>;

interface ToolConfirmationProps {
  sessionId: string;
  isClicked: boolean;
  actionRequiredContent: ActionRequired & { type: 'actionRequired' };
}

export default function ToolConfirmation({
  sessionId,
  isClicked,
  actionRequiredContent,
}: ToolConfirmationProps) {
  const intl = useIntl();
  const data = actionRequiredContent.data as ToolConfirmationData;
  const { id, toolName, prompt } = data;
  const displayName = formatToolName(toolName);

  return (
    <div className="goose-message-content bg-background-primary border border-border-primary rounded-2xl overflow-hidden">
      <div className="bg-background-secondary px-4 py-2 text-text-primary">
        {prompt
          ? intl.formatMessage(i18n.allowToolCallWithName, { toolName: displayName })
          : intl.formatMessage(i18n.gooseWouldLikeToCallWithName, { toolName: displayName })}
      </div>
      <ToolApprovalButtons
        data={{ id, toolName, prompt: prompt ?? undefined, sessionId, isClicked }}
      />
    </div>
  );
}
