import { useRef, useState } from 'react';
import { defineMessages, useIntl } from '../i18n';
import { formatAcpError, type AcpWorkingDirectoryMissingError } from '../acp/errors';
import { Button } from './ui/button';
import { Input } from './ui/input';

const i18n = defineMessages({
  missingDirectory: {
    id: 'workingDirectoryRecovery.missingDirectory',
    defaultMessage:
      'The working directory no longer exists: {path}. Choose a replacement directory to open this session.',
  },
  chooseDirectory: {
    id: 'dirSwitcher.chooseDirectory',
    defaultMessage: 'Choose directory…',
  },
  enterPath: {
    id: 'dirSwitcher.enterPath',
    defaultMessage: 'Enter path',
  },
  enterPathPlaceholder: {
    id: 'dirSwitcher.enterPathPlaceholder',
    defaultMessage: 'Enter an absolute path (e.g. /home/goose/workspace)',
  },
  retry: {
    id: 'baseChat.retry',
    defaultMessage: 'Retry',
  },
});

export function WorkingDirectoryRecovery({
  error,
  onReplace,
}: {
  error: AcpWorkingDirectoryMissingError;
  onReplace: (workingDir: string) => Promise<void>;
}) {
  const intl = useIntl();
  const [path, setPath] = useState('');
  const [busy, setBusy] = useState(false);
  const [pickerError, setPickerError] = useState<string>();
  const pending = useRef(false);

  const replaceDirectory = async (choose: boolean) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setPickerError(undefined);
    try {
      let replacement = path;
      if (choose) {
        const result = await window.electron.directoryChooser();
        if (result.canceled || result.filePaths.length === 0) return;
        replacement = result.filePaths[0];
        setPath(replacement);
      }
      await onReplace(replacement);
    } catch (failure) {
      setPickerError(formatAcpError(failure));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <p className="text-sm break-words">
        {intl.formatMessage(i18n.missingDirectory, { path: error.path })}
      </p>
      {(pickerError || error.recoveryError) && (
        <p role="alert" className="text-sm">
          {pickerError || error.recoveryError}
        </p>
      )}
      <Button variant="outline" disabled={busy} onClick={() => void replaceDirectory(true)}>
        {intl.formatMessage(i18n.chooseDirectory)}
      </Button>
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (path && !busy) void replaceDirectory(false);
        }}
      >
        <Input
          aria-label={intl.formatMessage(i18n.enterPath)}
          placeholder={intl.formatMessage(i18n.enterPathPlaceholder)}
          value={path}
          disabled={busy}
          onChange={(event) => setPath(event.target.value)}
        />
        <Button type="submit" variant="outline" disabled={busy || !path}>
          {intl.formatMessage(i18n.retry)}
        </Button>
      </form>
    </div>
  );
}
