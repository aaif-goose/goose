import { useState, useEffect, useRef } from 'react';
import { ActionRequired } from '../api';
import { defineMessages, useIntl } from '../i18n';
import { Button } from './ui/button';
import JsonSchemaForm from './ui/JsonSchemaForm';
import type { JsonSchema } from './ui/JsonSchemaForm';

const i18n = defineMessages({
  cancelled: {
    id: 'elicitationRequest.cancelled',
    defaultMessage: 'Information request was cancelled.',
  },
  submitted: {
    id: 'elicitationRequest.submitted',
    defaultMessage: 'Information submitted',
  },
  expired: {
    id: 'elicitationRequest.expired',
    defaultMessage: 'This request has expired. The extension will need to ask again.',
  },
  defaultMessage: {
    id: 'elicitationRequest.defaultMessage',
    defaultMessage: 'Goose needs some information from you.',
  },
  submit: {
    id: 'elicitationRequest.submit',
    defaultMessage: 'Submit',
  },
  accept: {
    id: 'elicitationRequest.accept',
    defaultMessage: 'Accept',
  },
  waitingForResponse: {
    id: 'elicitationRequest.waitingForResponse',
    defaultMessage: 'Waiting for your response ({timeRemaining} remaining)',
  },
});

const ELICITATION_TIMEOUT_SECONDS = 300;

interface ElicitationRequestProps {
  isCancelledMessage: boolean;
  isClicked: boolean;
  actionRequiredContent: ActionRequired & { type: 'actionRequired' };
  onSubmit: (elicitationId: string, userData: Record<string, unknown>) => void;
}

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

export default function ElicitationRequest({
  isCancelledMessage,
  isClicked,
  actionRequiredContent,
  onSubmit,
}: ElicitationRequestProps) {
  const intl = useIntl();
  const [submitted, setSubmitted] = useState(isClicked);
  const [timeRemaining, setTimeRemaining] = useState(ELICITATION_TIMEOUT_SECONDS);
  const startTimeRef = useRef(Date.now());

  useEffect(() => {
    if (submitted || isCancelledMessage || isClicked) return;

    const interval = setInterval(() => {
      const elapsed = Math.floor((Date.now() - startTimeRef.current) / 1000);
      const remaining = Math.max(0, ELICITATION_TIMEOUT_SECONDS - elapsed);
      setTimeRemaining(remaining);

      if (remaining === 0) {
        clearInterval(interval);
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [submitted, isCancelledMessage, isClicked]);

  if (actionRequiredContent.data.actionType !== 'elicitation') {
    return null;
  }

  const { id: elicitationId, message, requested_schema } = actionRequiredContent.data;

  const schema = (requested_schema ?? {}) as JsonSchema;
  const hasSchemaFields = Boolean(schema.properties && Object.keys(schema.properties).length > 0);

  const handleSubmit = (formData: Record<string, unknown>) => {
    setSubmitted(true);
    onSubmit(elicitationId, formData);
  };

  const handleAccept = () => {
    setSubmitted(true);
    onSubmit(elicitationId, {});
  };

  if (isCancelledMessage) {
    return (
      <div className="goose-message-content bg-background-secondary rounded-2xl px-4 py-2 text-text-primary">
        {intl.formatMessage(i18n.cancelled)}
      </div>
    );
  }

  if (submitted) {
    return (
      <div className="goose-message-content bg-background-secondary rounded-2xl px-4 py-2 text-text-primary">
        <div className="flex items-center gap-2">
          <svg
            className="w-5 h-5 text-gray-500"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
          </svg>
          <span>{intl.formatMessage(i18n.submitted)}</span>
        </div>
      </div>
    );
  }

  const isUrgent = timeRemaining <= 60;
  const isExpired = timeRemaining === 0;

  if (isExpired) {
    return (
      <div className="goose-message-content bg-background-secondary rounded-2xl px-4 py-2 text-text-primary">
        <div className="flex items-center gap-2 text-text-secondary">
          <svg
            className="w-5 h-5"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span>{intl.formatMessage(i18n.expired)}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      <div className="goose-message-content bg-background-secondary rounded-2xl rounded-b-none px-4 py-2 text-text-primary">
        <div className="flex justify-between items-start gap-4">
          <span>{message || intl.formatMessage(i18n.defaultMessage)}</span>
        </div>
      </div>
      <div className="goose-message-content bg-background-primary border border-border-primary dark:border-gray-700 rounded-b-2xl px-4 py-3">
        {hasSchemaFields ? (
          <JsonSchemaForm
            schema={schema}
            onSubmit={handleSubmit}
            submitLabel={intl.formatMessage(i18n.submit)}
          />
        ) : (
          <div className="flex gap-2">
            <Button type="button" onClick={handleAccept}>
              {intl.formatMessage(i18n.accept)}
            </Button>
          </div>
        )}
        <div
          className={`mt-3 pt-3 border-t border-border-primary flex items-center gap-2 text-sm ${isUrgent ? 'text-red-500' : 'text-text-secondary'}`}
        >
          <svg
            className="w-4 h-4 animate-pulse"
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span>
            {intl.formatMessage(i18n.waitingForResponse, {
              timeRemaining: formatTime(timeRemaining),
            })}
          </span>
        </div>
      </div>
    </div>
  );
}
