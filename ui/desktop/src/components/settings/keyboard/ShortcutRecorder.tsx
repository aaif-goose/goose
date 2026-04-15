import { useState, useEffect, useRef } from 'react';
import { Button } from '../../ui/button';
import { KeyboardShortcuts } from '../../../utils/settings';
import { getShortcutLabel, formatShortcut } from './KeyboardShortcutsSection';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  pressShortcut: {
    id: 'shortcutRecorder.pressShortcut',
    defaultMessage: 'Press shortcut...',
  },
  clickToRecord: {
    id: 'shortcutRecorder.clickToRecord',
    defaultMessage: 'Click to record...',
  },
  save: {
    id: 'shortcutRecorder.save',
    defaultMessage: 'Save',
  },
  cancel: {
    id: 'shortcutRecorder.cancel',
    defaultMessage: 'Cancel',
  },
  conflictWarning: {
    id: 'shortcutRecorder.conflictWarning',
    defaultMessage:
      'This shortcut is already used by {label}. Saving will reassign it to this action.',
  },
});

interface ShortcutRecorderProps {
  value: string;
  onSave: (shortcut: string) => void;
  onCancel: () => void;
  allShortcuts?: KeyboardShortcuts;
  currentKey?: keyof KeyboardShortcuts;
}

export function ShortcutRecorder({
  value,
  onSave,
  onCancel,
  allShortcuts,
  currentKey,
}: ShortcutRecorderProps) {
  const intl = useIntl();
  const [recording, setRecording] = useState(true);
  const [capturedShortcut, setCapturedShortcut] = useState(value);
  const [displayShortcut, setDisplayShortcut] = useState('');
  const [conflict, setConflict] = useState<string | null>(null);
  const inputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.focus();
    }
  }, []);

  useEffect(() => {
    if (recording && inputRef.current) {
      inputRef.current.focus();
    }
  }, [recording]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!recording) return;

    e.preventDefault();
    e.stopPropagation();

    // Ignore modifier-only presses
    if (['Control', 'Meta', 'Alt', 'Shift'].includes(e.key)) {
      return;
    }

    const parts: string[] = [];

    if (e.ctrlKey || e.metaKey) {
      parts.push('CommandOrControl');
    }
    if (e.altKey) {
      parts.push('Alt');
    }
    if (e.shiftKey) {
      parts.push('Shift');
    }

    let key = e.code && e.code.startsWith('Key') ? e.code.replace('Key', '') : e.key;

    const keyMap: Record<string, string> = {
      ' ': 'Space',
      Space: 'Space',
      ArrowUp: 'Up',
      ArrowDown: 'Down',
      ArrowLeft: 'Left',
      ArrowRight: 'Right',
      Escape: 'Esc',
      Delete: 'Delete',
      Backspace: 'Backspace',
      Tab: 'Tab',
      Enter: 'Return',
      Minus: '-',
      Equal: '=',
      BracketLeft: '[',
      BracketRight: ']',
      Backslash: '\\',
      Semicolon: ';',
      Quote: "'",
      Comma: ',',
      Period: '.',
      Slash: '/',
      Backquote: '`',
    };

    if (e.code && e.code.startsWith('Digit')) {
      key = e.code.replace('Digit', '');
    } else if (keyMap[key] || keyMap[e.code]) {
      key = keyMap[key] || keyMap[e.code];
    } else if (key.length === 1) {
      key = key.toUpperCase();
    }

    parts.push(key);

    const accelerator = parts.join('+');
    setCapturedShortcut(accelerator);

    if (allShortcuts && currentKey) {
      const conflictingKey = Object.entries(allShortcuts).find(
        ([key, shortcut]) => key !== currentKey && shortcut === accelerator
      );
      if (conflictingKey) {
        setConflict(conflictingKey[0]);
      } else {
        setConflict(null);
      }
    }

    setDisplayShortcut(formatShortcut(accelerator));
    setRecording(false);
  };

  const handleStartRecording = () => {
    setRecording(true);
    setCapturedShortcut('');
    setDisplayShortcut('');
    setConflict(null);
  };

  const handleSave = () => {
    onSave(capturedShortcut);
  };

  const handleCancel = () => {
    onCancel();
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <div
          ref={inputRef}
          tabIndex={0}
          onKeyDown={handleKeyDown}
          onClick={handleStartRecording}
          className={`
            text-xs font-mono px-3 py-2 rounded border
            ${
              recording
                ? 'bg-background-primary ring-1'
                : conflict
                  ? 'bg-background-secondary border-yellow-600/50'
                  : 'bg-background-secondary border-border-primary cursor-pointer'
            }
            focus:outline-none focus:ring-1
            w-64 text-center
          `}
        >
          {recording ? (
            <span className="text-text-secondary animate-pulse">
              {intl.formatMessage(i18n.pressShortcut)}
            </span>
          ) : displayShortcut ? (
            <span className={conflict ? 'text-yellow-600' : 'text-text-primary'}>
              {displayShortcut}
            </span>
          ) : capturedShortcut ? (
            <span className={conflict ? 'text-yellow-600' : 'text-text-primary'}>
              {formatShortcut(capturedShortcut)}
            </span>
          ) : (
            <span className="text-text-secondary">
              {intl.formatMessage(i18n.clickToRecord)}
            </span>
          )}
        </div>
        <Button
          variant="secondary"
          size="sm"
          onClick={handleSave}
          disabled={!capturedShortcut}
          className="text-xs"
        >
          {intl.formatMessage(i18n.save)}
        </Button>
        <Button variant="secondary" size="sm" onClick={handleCancel} className="text-xs">
          {intl.formatMessage(i18n.cancel)}
        </Button>
      </div>
      {conflict && (
        <div className="text-xs text-yellow-600 flex items-center gap-1">
          <span>⚠️</span>
          <span>
            {intl.formatMessage(i18n.conflictWarning, {
              label: getShortcutLabel(conflict, intl.formatMessage),
            })}
          </span>
        </div>
      )}
    </div>
  );
}
