import { useState, useEffect } from 'react';
import { Button } from './ui/button';
import type { Permission } from '../types/permissions';
import { resolveAcpPermissionRequest } from '../acp/permissionRequests';
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
  staleApprovalRequest: {
    id: 'toolApprovalButtons.staleApprovalRequest',
    defaultMessage: 'This approval request is no longer active.',
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
  generation?: string;
  id: string;
  toolName: string;
  prompt?: string;
  sessionId: string;
  isClicked?: boolean;
}

export default function ToolApprovalButtons({ data }: { data: ToolApprovalData }) {
  const intl = useIntl();
  const { generation, id, toolName, prompt, sessionId, isClicked: initialIsClicked } = data;
  const approvalStateKey = generation ?? id;

  const storedState = globalApprovalState.get(approvalStateKey);
  const [decision, setDecision] = useState<Permission | null>(storedState?.decision ?? null);
  const [isClicked, setIsClicked] = useState(storedState?.isClicked ?? initialIsClicked ?? false);
  const [approvalError, setApprovalError] = useState<string | null>(null);

  const setResolvedDecision = (action: Permission) => {
    setDecision(action);
    setIsClicked(true);
    setApprovalError(null);
  };

  useEffect(() => {
    const currentState = globalApprovalState.get(approvalStateKey);
    if (currentState) {
      setDecision(currentState.decision);
      setIsClicked(currentState.isClicked);
    } else {
      setDecision(null);
      setIsClicked(initialIsClicked ?? false);
    }
    setApprovalError(null);
  }, [approvalStateKey, initialIsClicked]);

  useEffect(() => {
    globalApprovalState.set(approvalStateKey, { decision, isClicked });
  }, [approvalStateKey, decision, isClicked]);

  const handleAction = async (action: Permission) => {
    try {
      if (resolveAcpPermissionRequest(sessionId, id, generation, action)) {
        setResolvedDecision(action);
      } else {
        setApprovalError(intl.formatMessage(i18n.staleApprovalRequest));
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
    <>
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
      {approvalError && (
        <p className="text-sm text-red-500 mt-2" role="alert">
          {approvalError}
        </p>
      )}
    </>
  );
}
