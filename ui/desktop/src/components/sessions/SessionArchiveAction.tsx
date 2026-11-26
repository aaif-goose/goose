import { useState } from 'react';
import { Archive, MoreHorizontal } from 'lucide-react';
import { toast } from 'react-toastify';
import { acpSetSessionArchived, type SessionListItem } from '../../acp/sessions';
import { AppEvents } from '../../constants/events';
import { defineMessages, useIntl } from '../../i18n';
import { errorMessage } from '../../utils/conversionUtils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

const messages = defineMessages({
  actions: { id: 'sessions.action.menu', defaultMessage: 'Chat actions' },
  archive: { id: 'sessions.action.archive', defaultMessage: 'Archive chat' },
  restore: { id: 'sessions.action.restore', defaultMessage: 'Restore chat' },
  archived: { id: 'sessions.archived', defaultMessage: 'Archived' },
  archiveFailed: {
    id: 'sessions.toast.archiveFailed',
    defaultMessage: 'Could not archive chat: {error}',
  },
  restoreFailed: {
    id: 'sessions.toast.restoreFailed',
    defaultMessage: 'Could not restore chat: {error}',
  },
});

interface SessionArchiveActionProps {
  session: SessionListItem;
  disabled: boolean;
}

export function SessionArchiveAction({ session, disabled }: SessionArchiveActionProps) {
  const intl = useIntl();
  const [pending, setPending] = useState(false);
  const archived = Boolean(session.archivedAt);

  const changeArchiveState = async () => {
    if (disabled || pending) return;

    setPending(true);
    try {
      await acpSetSessionArchived(session.id, !archived);
      window.dispatchEvent(
        new CustomEvent(AppEvents.SESSION_ARCHIVE_CHANGED, {
          detail: { sessionId: session.id, archived: !archived },
        })
      );
    } catch (error) {
      toast.error(
        intl.formatMessage(archived ? messages.restoreFailed : messages.archiveFailed, {
          error: errorMessage(error, 'Unknown error'),
        })
      );
    } finally {
      setPending(false);
    }
  };

  if (archived) {
    return (
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="rounded-full bg-background-secondary px-2 py-0.5 text-text-secondary">
          {intl.formatMessage(messages.archived)}
        </span>
        <button
          type="button"
          disabled={disabled || pending}
          className="rounded px-1 py-0.5 text-text-secondary hover:text-text-primary hover:underline disabled:opacity-50"
          onClick={(event) => {
            event.stopPropagation();
            void changeArchiveState();
          }}
        >
          {intl.formatMessage(messages.restore)}
        </button>
      </div>
    );
  }

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={intl.formatMessage(messages.actions)}
          disabled={pending}
          onClick={(event) => event.stopPropagation()}
          className="shrink-0 rounded p-0.5 text-text-secondary opacity-0 transition-opacity hover:bg-background-secondary group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
        >
          <MoreHorizontal className="size-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" onClick={(event) => event.stopPropagation()}>
        <DropdownMenuItem disabled={disabled} onSelect={() => void changeArchiveState()}>
          <Archive />
          {intl.formatMessage(messages.archive)}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
