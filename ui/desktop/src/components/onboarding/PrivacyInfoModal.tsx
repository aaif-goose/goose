import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  title: {
    id: 'privacyInfoModal.title',
    defaultMessage: 'Privacy details',
  },
  description: {
    id: 'privacyInfoModal.description',
    defaultMessage: 'Anonymous usage data helps us understand how goose is used and identify areas for improvement.',
  },
  whatWeCollect: {
    id: 'privacyInfoModal.whatWeCollect',
    defaultMessage: 'What we collect:',
  },
  collectOs: {
    id: 'privacyInfoModal.collectOs',
    defaultMessage: 'Operating system, version, and architecture',
  },
  collectVersion: {
    id: 'privacyInfoModal.collectVersion',
    defaultMessage: 'goose version and install method',
  },
  collectProvider: {
    id: 'privacyInfoModal.collectProvider',
    defaultMessage: 'Provider and model used',
  },
  collectExtensions: {
    id: 'privacyInfoModal.collectExtensions',
    defaultMessage: 'Extensions and tool usage counts (names only)',
  },
  collectSession: {
    id: 'privacyInfoModal.collectSession',
    defaultMessage: 'Session metrics (duration, interaction count, token usage)',
  },
  collectErrors: {
    id: 'privacyInfoModal.collectErrors',
    defaultMessage: 'Error types (e.g., "rate_limit", "auth" - no details)',
  },
  neverCollect: {
    id: 'privacyInfoModal.neverCollect',
    defaultMessage: 'We never collect your conversations, code, tool arguments, error messages, or any personal data. You can change this setting anytime in Settings.',
  },
});

interface PrivacyInfoModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PrivacyInfoModal({ isOpen, onClose }: PrivacyInfoModalProps) {
  const intl = useIntl();

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="w-[440px]">
        <DialogHeader>
          <DialogTitle className="text-center">{intl.formatMessage(i18n.title)}</DialogTitle>
        </DialogHeader>

        <div>
          <p className="text-text-muted text-sm mb-3">
            {intl.formatMessage(i18n.description)}
          </p>
          <p className="font-medium text-text-default text-sm mb-1.5">{intl.formatMessage(i18n.whatWeCollect)}</p>
          <ul className="text-text-muted text-sm list-disc list-outside space-y-0.5 ml-5 mb-3">
            <li>{intl.formatMessage(i18n.collectOs)}</li>
            <li>{intl.formatMessage(i18n.collectVersion)}</li>
            <li>{intl.formatMessage(i18n.collectProvider)}</li>
            <li>{intl.formatMessage(i18n.collectExtensions)}</li>
            <li>{intl.formatMessage(i18n.collectSession)}</li>
            <li>{intl.formatMessage(i18n.collectErrors)}</li>
          </ul>
          <p className="text-text-muted text-sm">
            {intl.formatMessage(i18n.neverCollect)}
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
