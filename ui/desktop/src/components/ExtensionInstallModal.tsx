import { useState, useCallback, useEffect, useRef } from 'react';
import { IpcRendererEvent } from 'electron';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { extractExtensionName } from './settings/extensions/utils';
import { addExtensionFromDeepLink } from './settings/extensions/deeplink';
import type { ExtensionConfig } from '../api/types.gen';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import { toastService } from '../toasts';
import { errorMessage } from '../utils/conversionUtils';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  unknownCommand: {
    id: 'extensionInstallModal.unknownCommand',
    defaultMessage: 'Unknown Command',
  },
  blockedTitle: {
    id: 'extensionInstallModal.blockedTitle',
    defaultMessage: 'Extension Installation Blocked',
  },
  blockedMessage: {
    id: 'extensionInstallModal.blockedMessage',
    defaultMessage: 'This extension command is not in the allowed list and its installation is blocked.\n\nExtension: {name}\nCommand: {command}\n\nContact your administrator to request approval for this extension.',
  },
  ok: {
    id: 'extensionInstallModal.ok',
    defaultMessage: 'OK',
  },
  untrustedTitle: {
    id: 'extensionInstallModal.untrustedTitle',
    defaultMessage: 'Install Untrusted Extension?',
  },
  untrustedSecurityMessage: {
    id: 'extensionInstallModal.untrustedSecurityMessage',
    defaultMessage: 'This extension command is not in the allowed list and will be able to access your conversations and provide additional functionality.\n\nInstalling extensions from untrusted sources may pose security risks.',
  },
  untrustedMessageWithUrl: {
    id: 'extensionInstallModal.untrustedMessageWithUrl',
    defaultMessage: '{securityMessage}\n\nExtension: {name}\nURL: {url}\n\nContact your administrator if you are unsure about this.',
  },
  untrustedMessageWithCommand: {
    id: 'extensionInstallModal.untrustedMessageWithCommand',
    defaultMessage: '{securityMessage}\n\nExtension: {name}\nCommand: {command}\n\nContact your administrator if you are unsure about this.',
  },
  installAnyway: {
    id: 'extensionInstallModal.installAnyway',
    defaultMessage: 'Install Anyway',
  },
  cancel: {
    id: 'extensionInstallModal.cancel',
    defaultMessage: 'Cancel',
  },
  trustedTitle: {
    id: 'extensionInstallModal.trustedTitle',
    defaultMessage: 'Confirm Extension Installation',
  },
  trustedMessage: {
    id: 'extensionInstallModal.trustedMessage',
    defaultMessage: 'Are you sure you want to install the {name} extension?\n\nCommand: {command}',
  },
  yes: {
    id: 'extensionInstallModal.yes',
    defaultMessage: 'Yes',
  },
  no: {
    id: 'extensionInstallModal.no',
    defaultMessage: 'No',
  },
  alreadyInstalledTitle: {
    id: 'extensionInstallModal.alreadyInstalledTitle',
    defaultMessage: "Extension ''{name}'' Already Installed",
  },
  alreadyInstalledMessage: {
    id: 'extensionInstallModal.alreadyInstalledMessage',
    defaultMessage: "''{name}'' extension has already been installed successfully. Start a new chat session to use it.",
  },
  installing: {
    id: 'extensionInstallModal.installing',
    defaultMessage: 'Installing...',
  },
});

type ModalType = 'blocked' | 'untrusted' | 'trusted';

interface ExtensionInfo {
  name: string;
  command?: string;
  remoteUrl?: string;
  link: string;
}

interface ExtensionModalState {
  isOpen: boolean;
  modalType: ModalType;
  extensionInfo: ExtensionInfo | null;
  isPending: boolean;
  error: string | null;
}

interface ExtensionModalConfig {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  showSingleButton: boolean;
  isBlocked: boolean;
}

interface ExtensionInstallModalProps {
  addExtension?: (name: string, config: ExtensionConfig, enabled: boolean) => Promise<void>;
  setView: (view: View, options?: ViewOptions) => void;
}

function extractCommand(link: string): string {
  const url = new URL(link);

  // For remote extensions (SSE or Streaming HTTP), return the URL
  const remoteUrl = url.searchParams.get('url');
  if (remoteUrl) {
    return remoteUrl;
  }

  // For stdio extensions, return the command
  const cmd = url.searchParams.get('cmd') || '';
  const args = url.searchParams.getAll('arg').map(decodeURIComponent);
  return `${cmd} ${args.join(' ')}`.trim();
}

function extractRemoteUrl(link: string): string | null {
  const url = new URL(link);
  return url.searchParams.get('url');
}

export function ExtensionInstallModal({ addExtension, setView }: ExtensionInstallModalProps) {
  const intl = useIntl();
  const { getExtensions } = useConfig();
  const getExtensionsRef = useRef(getExtensions);
  const processingLinkRef = useRef<string | null>(null);

  useEffect(() => {
    getExtensionsRef.current = getExtensions;
  }, [getExtensions]);

  const [modalState, setModalState] = useState<ExtensionModalState>({
    isOpen: false,
    modalType: 'trusted',
    extensionInfo: null,
    isPending: false,
    error: null,
  });

  const [pendingLink, setPendingLink] = useState<string | null>(null);

  const determineModalType = async (
    command: string,
    _remoteUrl: string | null
  ): Promise<ModalType> => {
    try {
      const config = window.electron.getConfig();
      const ALLOWLIST_WARNING_MODE = config.GOOSE_ALLOWLIST_WARNING === true;

      if (ALLOWLIST_WARNING_MODE) {
        return 'untrusted';
      }

      const allowedCommands = await window.electron.getAllowedExtensions();

      if (!allowedCommands || allowedCommands.length === 0) {
        return 'trusted';
      }

      const isCommandAllowed = allowedCommands.some((allowedCmd: string) =>
        command.startsWith(allowedCmd)
      );

      return isCommandAllowed ? 'trusted' : 'blocked';
    } catch (error) {
      console.error('Error checking allowlist:', error);
      return 'trusted';
    }
  };

  const generateModalConfig = (
    modalType: ModalType,
    extensionInfo: ExtensionInfo
  ): ExtensionModalConfig => {
    const { name, command, remoteUrl } = extensionInfo;
    const displayCommand = command || remoteUrl || intl.formatMessage(i18n.unknownCommand);

    switch (modalType) {
      case 'blocked':
        return {
          title: intl.formatMessage(i18n.blockedTitle),
          message: '\n\n' + intl.formatMessage(i18n.blockedMessage, { name, command: displayCommand }),
          confirmLabel: intl.formatMessage(i18n.ok),
          cancelLabel: '',
          showSingleButton: true,
          isBlocked: true,
        };

      case 'untrusted': {
        const securityMessage = '\n\n' + intl.formatMessage(i18n.untrustedSecurityMessage);
        const message = remoteUrl
          ? intl.formatMessage(i18n.untrustedMessageWithUrl, { securityMessage, name, url: remoteUrl })
          : intl.formatMessage(i18n.untrustedMessageWithCommand, { securityMessage, name, command: displayCommand });

        return {
          title: intl.formatMessage(i18n.untrustedTitle),
          message,
          confirmLabel: intl.formatMessage(i18n.installAnyway),
          cancelLabel: intl.formatMessage(i18n.cancel),
          showSingleButton: false,
          isBlocked: false,
        };
      }

      case 'trusted':
      default:
        return {
          title: intl.formatMessage(i18n.trustedTitle),
          message: intl.formatMessage(i18n.trustedMessage, { name, command: displayCommand }),
          confirmLabel: intl.formatMessage(i18n.yes),
          cancelLabel: intl.formatMessage(i18n.no),
          showSingleButton: false,
          isBlocked: false,
        };
    }
  };

  const handleExtensionRequest = useCallback(async (link: string): Promise<void> => {
    if (processingLinkRef.current === link) {
      return;
    }
    processingLinkRef.current = link;

    try {

      const command = extractCommand(link);
      const remoteUrl = extractRemoteUrl(link);
      const extName = extractExtensionName(link);
      const extensionsList = await getExtensionsRef.current(true);

      if (extensionsList?.find((ext) => ext.name === extName)) {

        toastService.success({
          title: intl.formatMessage(i18n.alreadyInstalledTitle, { name: extName }),
          msg: intl.formatMessage(i18n.alreadyInstalledMessage, { name: extName }),
        });
        return;
      }

      const extensionInfo: ExtensionInfo = {
        name: extName,
        command: command,
        remoteUrl: remoteUrl || undefined,
        link: link,
      };

      const modalType = await determineModalType(command, remoteUrl);

      setModalState({
        isOpen: true,
        modalType,
        extensionInfo,
        isPending: false,
        error: null,
      });

      setPendingLink(modalType === 'blocked' ? null : link);

      window.electron.logInfo(`Extension modal opened: ${modalType} for ${extName}`);
    } catch (error) {
      console.error('Error processing extension request:', error);
      setModalState((prev) => ({
        ...prev,
        error: errorMessage(error, 'Unknown error'),
      }));
    } finally {
      processingLinkRef.current = null;
    }
  }, [intl]);

  const dismissModal = useCallback(() => {
    setModalState({
      isOpen: false,
      modalType: 'trusted',
      extensionInfo: null,
      isPending: false,
      error: null,
    });
    setPendingLink(null);
  }, []);

  const confirmInstall = useCallback(async (): Promise<void> => {
    if (!pendingLink) {
      return;
    }

    setModalState((prev) => ({ ...prev, isPending: true }));

    try {

      if (addExtension) {
        await addExtensionFromDeepLink(
          pendingLink,
          addExtension,
          (view: string, options?: ViewOptions) => {
            setView(view as View, options);
          }
        );
      } else {
        throw new Error('addExtension function not provided to component');
      }
      dismissModal();
    } catch (error) {
      setModalState((prev) => ({
        ...prev,
        error: errorMessage(error, 'Installation failed'),
        isPending: false,
      }));
    }
  }, [pendingLink, dismissModal, addExtension, setView]);

  useEffect(() => {

    const handleAddExtension = async (_event: IpcRendererEvent, ...args: unknown[]) => {
      const link = args[0] as string;
      await handleExtensionRequest(link);
    };

    window.electron.on('add-extension', handleAddExtension);

    return () => {
      window.electron.off('add-extension', handleAddExtension);
    };
  }, [handleExtensionRequest]);

  const getModalConfig = (): ExtensionModalConfig | null => {
    if (!modalState.extensionInfo) return null;
    return generateModalConfig(modalState.modalType, modalState.extensionInfo);
  };

  const config = getModalConfig();
  if (!config) return null;

  const getConfirmButtonVariant = () => {
    switch (modalState.modalType) {
      case 'blocked':
        return 'outline';
      case 'untrusted':
        return 'destructive';
      case 'trusted':
      default:
        return 'default';
    }
  };

  const getTitleClassName = () => {
    switch (modalState.modalType) {
      case 'blocked':
        return 'text-red-600 dark:text-red-400';
      case 'untrusted':
        return 'text-yellow-600 dark:text-yellow-400';
      case 'trusted':
      default:
        return '';
    }
  };

  return (
    <Dialog open={modalState.isOpen} onOpenChange={(open) => !open && dismissModal()}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className={getTitleClassName()}>{config.title}</DialogTitle>
          <DialogDescription className="text-left whitespace-pre-wrap">
            {config.message}
          </DialogDescription>
        </DialogHeader>

        <DialogFooter className="pt-4">
          {config.showSingleButton ? (
            <Button
              onClick={dismissModal}
              disabled={modalState.isPending}
              variant={getConfirmButtonVariant()}
            >
              {config.confirmLabel}
            </Button>
          ) : (
            <>
              <Button variant="outline" onClick={dismissModal} disabled={modalState.isPending}>
                {config.cancelLabel}
              </Button>
              <Button
                onClick={confirmInstall}
                disabled={modalState.isPending}
                variant={getConfirmButtonVariant()}
              >
                {modalState.isPending ? intl.formatMessage(i18n.installing) : config.confirmLabel}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
