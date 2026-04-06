import { ActionRequired } from '../api';
import { defineMessages, useIntl } from '../i18n';
import ToolApprovalButtons from './ToolApprovalButtons';

const i18n = defineMessages({
  allowToolCall: {
    id: 'toolConfirmation.allowToolCall',
    defaultMessage: 'Do you allow this tool call?',
  },
  gooseWouldLikeToCall: {
    id: 'toolConfirmation.gooseWouldLikeToCall',
    defaultMessage: 'Goose would like to call the above tool. Allow?',
  },
});

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

  return (
    <div className="goose-message-content bg-background-primary border border-border-primary rounded-2xl overflow-hidden">
      <div className="bg-background-secondary px-4 py-2 text-text-primary">
        {prompt
          ? intl.formatMessage(i18n.allowToolCall)
          : intl.formatMessage(i18n.gooseWouldLikeToCall)}
      </div>
      <ToolApprovalButtons
        data={{ id, toolName, prompt: prompt ?? undefined, sessionId, isClicked }}
      />
    </div>
  );
}
