import { useCallback, useEffect, useRef, useState } from 'react';
import { CheckCircle2, CircleAlert, LoaderCircle, RefreshCw } from 'lucide-react';
import { acpEnableProvider, acpRefreshProviderDetails } from '../../../acp/providers';
import { defineMessages, useIntl } from '../../../i18n';
import type { ProviderDetails } from '../../../types/providers';
import { errorMessage } from '../../../utils/conversionUtils';
import { Button } from '../../ui/button';

const i18n = defineMessages({
  adapterFound: {
    id: 'acpReadinessPanel.adapterFound',
    defaultMessage: 'ACP adapter found',
  },
  adapterNotFound: {
    id: 'acpReadinessPanel.adapterNotFound',
    defaultMessage: 'ACP adapter not found',
  },
  connected: {
    id: 'acpReadinessPanel.connected',
    defaultMessage: 'Connected successfully',
  },
  connectionNotChecked: {
    id: 'acpReadinessPanel.connectionNotChecked',
    defaultMessage: 'Check your account connection before continuing.',
  },
  authenticationHelp: {
    id: 'acpReadinessPanel.authenticationHelp',
    defaultMessage: 'Sign in through the provider CLI, then check again.',
  },
  checkAgain: {
    id: 'acpReadinessPanel.checkAgain',
    defaultMessage: 'Check again',
  },
  checking: {
    id: 'acpReadinessPanel.checking',
    defaultMessage: 'Checking...',
  },
});

function setupStep(text: string) {
  return text.split(/(https?:\/\/[^\s]+)/g).map((part, index) =>
    /^https?:\/\//.test(part) ? (
      <a
        key={index}
        href="#"
        onClick={(event) => {
          event.preventDefault();
          window.electron.openExternal(part);
        }}
        className="underline hover:text-text-primary"
      >
        {part}
      </a>
    ) : (
      part
    )
  );
}

function wasAborted(error: unknown) {
  return error instanceof DOMException && error.name === 'AbortError';
}

export default function AcpReadinessPanel({
  provider,
  actionLabel,
  removeLabel,
  onConfigured,
  onRemove,
  onError,
}: {
  provider: ProviderDetails;
  actionLabel: string;
  removeLabel?: string;
  onConfigured: (provider: ProviderDetails) => void | Promise<void>;
  onRemove?: () => void;
  onError: (message: string | null) => void;
}) {
  const intl = useIntl();
  const [status, setStatus] = useState(provider);
  const [isChecking, setIsChecking] = useState(false);
  const [connectionChecked, setConnectionChecked] = useState(false);
  const [readinessError, setReadinessError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  const setupSteps = provider.metadata.setup_steps ?? [];
  const canContinue = !isChecking && status.is_available && connectionChecked && !readinessError;

  const startRequest = useCallback(() => {
    request.current?.abort();
    const controller = new AbortController();
    request.current = controller;
    return controller;
  }, []);

  const check = useCallback(async () => {
    const controller = startRequest();
    setIsChecking(true);
    setConnectionChecked(false);
    setReadinessError(null);
    onError(null);
    try {
      const result = await acpRefreshProviderDetails(provider.name, controller.signal);
      setStatus(result.provider);
      setConnectionChecked(result.connectionChecked);
      setReadinessError(result.readinessError);
    } catch (error) {
      if (!wasAborted(error)) onError(errorMessage(error));
    } finally {
      if (!controller.signal.aborted) setIsChecking(false);
    }
  }, [onError, provider.name, startRequest]);

  useEffect(() => {
    void check();
    return () => request.current?.abort();
  }, [check]);

  const configure = async () => {
    const controller = startRequest();
    setIsChecking(true);
    onError(null);
    try {
      const configured = await acpEnableProvider(provider.name, controller.signal);
      await onConfigured(configured);
    } catch (error) {
      if (!wasAborted(error)) onError(errorMessage(error));
    } finally {
      if (!controller.signal.aborted) setIsChecking(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-md border border-border-primary p-3 space-y-2">
        <div className="flex items-center gap-2 text-sm font-medium">
          {isChecking ? (
            <LoaderCircle className="h-4 w-4 animate-spin" />
          ) : status.is_available ? (
            <CheckCircle2 className="h-4 w-4 text-green-600" />
          ) : (
            <CircleAlert className="h-4 w-4 text-yellow-600" />
          )}
          {isChecking
            ? intl.formatMessage(i18n.checking)
            : intl.formatMessage(status.is_available ? i18n.adapterFound : i18n.adapterNotFound)}
        </div>
        {connectionChecked && !readinessError && (
          <div className="text-sm text-text-secondary">{intl.formatMessage(i18n.connected)}</div>
        )}
        {!isChecking && status.is_available && !connectionChecked && !readinessError && (
          <div className="text-sm text-text-secondary">
            {intl.formatMessage(i18n.connectionNotChecked)}
          </div>
        )}
        {readinessError && (
          <div className="space-y-1 text-sm text-red-600 break-words">
            <div>{readinessError}</div>
            <div>{intl.formatMessage(i18n.authenticationHelp)}</div>
          </div>
        )}
        {connectionChecked && !readinessError && status.last_refresh_error && (
          <div className="text-sm text-yellow-600 break-words">{status.last_refresh_error}</div>
        )}
      </div>

      {(!status.is_available || readinessError) && setupSteps.length > 0 && (
        <ol className="ml-5 list-decimal text-sm text-text-muted space-y-1">
          {setupSteps.map((step, index) => (
            <li key={index}>{setupStep(step)}</li>
          ))}
        </ol>
      )}

      <div className="flex justify-end gap-2 border-t border-border-primary pt-4">
        {!isChecking && (
          <Button type="button" variant="outline" onClick={check}>
            <RefreshCw className="mr-2 h-4 w-4" />
            {intl.formatMessage(i18n.checkAgain)}
          </Button>
        )}
        {canContinue && (
          <Button type="button" onClick={configure}>
            {actionLabel}
          </Button>
        )}
        {status.is_configured && onRemove && removeLabel && (
          <Button type="button" variant="destructive" onClick={onRemove}>
            {removeLabel}
          </Button>
        )}
      </div>
    </div>
  );
}
