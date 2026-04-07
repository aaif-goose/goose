import { useState, useEffect } from 'react';
import { Button } from './ui/button';
import { confirmToolAction, Permission } from '../api';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  allowOnce: {
    id: 'toolApprovalButtons.allowOnce',
    defaultMessage: 'Allow Once',
  },
  alwaysAllow: {
    id: 'toolApprovalButtons.alwaysAllow',
    defaultMessage: 'Always Allow',
  },
  deny: {
    id: 'toolApprovalButtons.deny',
    defaultMessage: 'Deny',
  },
  allowedOnce: {
    id: 'toolApprovalButtons.allowedOnce',
    defaultMessage: 'Allowed once',
  },
  alwaysAllowed: {
    id: 'toolApprovalButtons.alwaysAllowed',
    defaultMessage: 'Always allowed',
  },
  denied: {
    id: 'toolApprovalButtons.denied',
    defaultMessage: 'Denied',
  },
  deniedOnce: {
    id: 'toolApprovalButtons.deniedOnce',
    defaultMessage: 'Denied once',
  },
  cancelled: {
    id: 'toolApprovalButtons.cancelled',
    defaultMessage: 'Cancelled',
  },
});

const globalApprovalState = new Map<
  string,
  {
    decision: Permission | null;
    isClicked: boolean;
  }
>();

export interface ToolApprovalData {
  id: string;
  toolName: string;
  prompt?: string;
  sessionId: string;
  isClicked?: boolean;
}

export default function ToolApprovalButtons({ data }: { data: ToolApprovalData }) {
  const intl = useIntl();
  const { id, toolName, prompt, sessionId, isClicked: initialIsClicked } = data;

  const storedState = globalApprovalState.get(id);
  const [decision, setDecision] = useState<Permission | null>(storedState?.decision ?? null);
  const [isClicked, setIsClicked] = useState(storedState?.isClicked ?? initialIsClicked ?? false);

  useEffect(() => {
    const currentState = globalApprovalState.get(id);
    if (currentState) {
      setDecision(currentState.decision);
      setIsClicked(currentState.isClicked);
    }
  }, [id]);

  useEffect(() => {
    globalApprovalState.set(id, { decision, isClicked });
  }, [id, decision, isClicked]);

  const handleAction = async (action: Permission) => {
    setDecision(action);
    setIsClicked(true);

    try {
      const response = await confirmToolAction({
        body: {
          sessionId,
          id,
          action,
          principalType: 'Tool',
        },
      });
      if (response.error) {
        console.error('Failed to confirm tool action:', response.error);
      }
    } catch (err) {
      console.error('Error confirming tool action:', err);
    }
  };

  if (isClicked && decision) {
    const statusMessages: Record<Permission, string> = {
      allow_once: intl.formatMessage(i18n.allowedOnce),
      always_allow: intl.formatMessage(i18n.alwaysAllowed),
      always_deny: intl.formatMessage(i18n.denied),
      deny_once: intl.formatMessage(i18n.deniedOnce),
      cancel: intl.formatMessage(i18n.cancelled),
    };
    return (
      <p className="text-sm text-muted-foreground mt-2">
        {toolName} - {statusMessages[decision]}
      </p>
    );
  }

  return (
    <div className="flex items-center gap-2 mt-2">
      <Button
        className="rounded-full"
        variant="secondary"
        onClick={() => handleAction('allow_once')}
      >
        {intl.formatMessage(i18n.allowOnce)}
      </Button>
      {!prompt && (
        <Button
          className="rounded-full"
          variant="secondary"
          onClick={() => handleAction('always_allow')}
        >
          {intl.formatMessage(i18n.alwaysAllow)}
        </Button>
      )}
      <Button className="rounded-full" variant="outline" onClick={() => handleAction('deny_once')}>
        {intl.formatMessage(i18n.deny)}
      </Button>
    </div>
  );
}
