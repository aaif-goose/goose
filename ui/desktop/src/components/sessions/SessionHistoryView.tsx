import React, { useState, useEffect } from 'react';
import {
  Calendar,
  MessageSquareText,
  Folder,
  Share2,
  Sparkles,
  Copy,
  Check,
  Target,
  LoaderCircle,
  AlertCircle,
} from 'lucide-react';
import { defineMessages, useIntl } from '../../i18n';
import { resumeSession } from '../../sessions';
import { Button } from '../ui/button';
import { toast } from 'react-toastify';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { ScrollArea } from '../ui/scroll-area';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { createSharedSession } from '../../sharedSessions';
import { errorMessage } from '../../utils/conversionUtils';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import ProgressiveMessageList from '../ProgressiveMessageList';
import { SearchView } from '../conversation/SearchView';
import BackButton from '../ui/BackButton';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { Message, Session } from '../../api';
import { useNavigation } from '../../hooks/useNavigation';

const i18n = defineMessages({
  errorLoadingDetails: {
    id: 'sessionHistory.error.loading',
    defaultMessage: 'Error Loading Session Details',
  },
  tryAgain: {
    id: 'sessionHistory.error.tryAgain',
    defaultMessage: 'Try Again',
  },
  searchPlaceholder: {
    id: 'sessionHistory.searchPlaceholder',
    defaultMessage: 'Search history...',
  },
  noMessages: {
    id: 'sessionHistory.empty.title',
    defaultMessage: 'No messages found',
  },
  noMessagesDesc: {
    id: 'sessionHistory.empty.description',
    defaultMessage: "This session doesn't contain any messages",
  },
  loadingDetails: {
    id: 'sessionHistory.loading',
    defaultMessage: 'Loading session details...',
  },
  sharing: {
    id: 'sessionHistory.sharing',
    defaultMessage: 'Sharing...',
  },
  share: {
    id: 'sessionHistory.share',
    defaultMessage: 'Share',
  },
  shareTooltip: {
    id: 'sessionHistory.shareTooltip',
    defaultMessage: 'To enable session sharing, go to <b>Settings</b> > <b>Session</b> > <b>Session Sharing</b>.',
  },
  resume: {
    id: 'sessionHistory.resume',
    defaultMessage: 'Resume',
  },
  shareSessionTitle: {
    id: 'sessionHistory.shareModal.title',
    defaultMessage: 'Share Session (beta)',
  },
  shareSessionDescription: {
    id: 'sessionHistory.shareModal.description',
    defaultMessage: 'Share this session link to give others a read only view of your goose chat.',
  },
  copy: {
    id: 'sessionHistory.copy',
    defaultMessage: 'Copy',
  },
  cancel: {
    id: 'sessionHistory.cancel',
    defaultMessage: 'Cancel',
  },
  failedToShare: {
    id: 'sessionHistory.toast.shareFailed',
    defaultMessage: 'Failed to share session: {error}',
  },
  failedToCopy: {
    id: 'sessionHistory.toast.copyFailed',
    defaultMessage: 'Failed to copy link to clipboard',
  },
  couldNotLaunch: {
    id: 'sessionHistory.toast.launchFailed',
    defaultMessage: 'Could not launch session: {error}',
  },
});

const isUserMessage = (message: Message): boolean => {
  if (message.role === 'assistant') {
    return false;
  }
  return !message.content.every(
    (c) => c.type === 'actionRequired' && c.data.actionType === 'toolConfirmation'
  );
};

const filterMessagesForDisplay = (messages: Message[]): Message[] => {
  return messages;
};

interface SessionHistoryViewProps {
  session: Session;
  isLoading: boolean;
  error: string | null;
  onBack: () => void;
  onRetry: () => void;
  showActionButtons?: boolean;
}

// Custom SessionHeader component similar to SessionListView style
const SessionHeader: React.FC<{
  onBack: () => void;
  children: React.ReactNode;
  title: string;
  actionButtons?: React.ReactNode;
}> = ({ onBack, children, title, actionButtons }) => {
  return (
    <div className="flex flex-col pb-8 border-b">
      <div className="flex items-center pt-0 mb-1">
        <BackButton onClick={onBack} />
      </div>
      <h1 className="text-4xl font-light mb-4 pt-6">{title}</h1>
      <div className="flex items-center">{children}</div>
      {actionButtons && <div className="flex items-center space-x-3 mt-4">{actionButtons}</div>}
    </div>
  );
};

const SessionMessages: React.FC<{
  messages: Message[];
  isLoading: boolean;
  error: string | null;
  onRetry: () => void;
}> = ({ messages, isLoading, error, onRetry }) => {
  const intl = useIntl();
  const filteredMessages = filterMessagesForDisplay(messages);

  return (
    <ScrollArea className="h-full w-full">
      <div className="pb-24 pt-8">
        <div className="flex flex-col space-y-6">
          {isLoading ? (
            <div className="flex justify-center items-center py-12">
              <LoaderCircle className="animate-spin h-8 w-8 text-text-primary" />
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center py-8 text-text-secondary">
              <div className="text-red-500 mb-4">
                <AlertCircle size={32} />
              </div>
              <p className="text-md mb-2">{intl.formatMessage(i18n.errorLoadingDetails)}</p>
              <p className="text-sm text-center mb-4">{error}</p>
              <Button onClick={onRetry} variant="default">
                {intl.formatMessage(i18n.tryAgain)}
              </Button>
            </div>
          ) : filteredMessages?.length > 0 ? (
            <div className="max-w-4xl mx-auto w-full">
              <SearchView placeholder={intl.formatMessage(i18n.searchPlaceholder)}>
                <ProgressiveMessageList
                  messages={filteredMessages}
                  chat={{
                    sessionId: 'session-preview',
                  }}
                  toolCallNotifications={new Map()}
                  append={() => {}} // Read-only for session history
                  isUserMessage={isUserMessage} // Use the same function as BaseChat
                  batchSize={15} // Same as BaseChat default
                  batchDelay={30} // Same as BaseChat default
                  showLoadingThreshold={30} // Same as BaseChat default
                />
              </SearchView>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center py-8 text-text-secondary">
              <MessageSquareText className="w-12 h-12 mb-4" />
              <p className="text-lg mb-2">{intl.formatMessage(i18n.noMessages)}</p>
              <p className="text-sm">{intl.formatMessage(i18n.noMessagesDesc)}</p>
            </div>
          )}
        </div>
      </div>
    </ScrollArea>
  );
};

const SessionHistoryView: React.FC<SessionHistoryViewProps> = ({
  session,
  isLoading,
  error,
  onBack,
  onRetry,
  showActionButtons = true,
}) => {
  const [isShareModalOpen, setIsShareModalOpen] = useState(false);
  const [shareLink, setShareLink] = useState<string>('');
  const [isSharing, setIsSharing] = useState(false);
  const [isCopied, setIsCopied] = useState(false);
  const [canShare, setCanShare] = useState(false);

  const intl = useIntl();
  const messages = session.conversation || [];

  const setView = useNavigation();

  useEffect(() => {
    window.electron.getSetting('sessionSharing').then((config) => {
      if (config.enabled && config.baseUrl) {
        setCanShare(true);
      }
    });
  }, []);

  const handleShare = async () => {
    setIsSharing(true);

    try {
      const config = await window.electron.getSetting('sessionSharing');
      if (!config.enabled || !config.baseUrl) {
        throw new Error('Session sharing is not enabled or base URL is not configured.');
      }

      const shareToken = await createSharedSession(
        config.baseUrl,
        session.working_dir,
        messages,
        session.name || 'Shared Session',
        session.total_tokens || 0
      );

      const shareableLink = `goose://sessions/${shareToken}`;
      setShareLink(shareableLink);
      setIsShareModalOpen(true);
    } catch (error) {
      console.error('Error sharing session:', error);
      toast.error(intl.formatMessage(i18n.failedToShare, { error: errorMessage(error, 'Unknown error') }));
    } finally {
      setIsSharing(false);
    }
  };

  const handleCopyLink = () => {
    navigator.clipboard
      .writeText(shareLink)
      .then(() => {
        setIsCopied(true);
        setTimeout(() => setIsCopied(false), 2000);
      })
      .catch((err) => {
        console.error('Failed to copy link:', err);
        toast.error(intl.formatMessage(i18n.failedToCopy));
      });
  };

  const handleResumeSession = () => {
    try {
      resumeSession(session, setView);
    } catch (error) {
      toast.error(intl.formatMessage(i18n.couldNotLaunch, { error: errorMessage(error) }));
    }
  };

  const actionButtons = showActionButtons ? (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            onClick={handleShare}
            disabled={!canShare || isSharing}
            size="sm"
            variant="outline"
            className={canShare ? '' : 'cursor-not-allowed opacity-50'}
          >
            {isSharing ? (
              <>
                <LoaderCircle className="w-4 h-4 mr-2 animate-spin" />
                {intl.formatMessage(i18n.sharing)}
              </>
            ) : (
              <>
                <Share2 className="w-4 h-4" />
                {intl.formatMessage(i18n.share)}
              </>
            )}
          </Button>
        </TooltipTrigger>
        {!canShare ? (
          <TooltipContent>
            <p>
              {intl.formatMessage(i18n.shareTooltip, {
                b: (chunks: React.ReactNode) => <b>{chunks}</b>,
              })}
            </p>
          </TooltipContent>
        ) : null}
      </Tooltip>
      <Button onClick={handleResumeSession} size="sm" variant="outline">
        <Sparkles className="w-4 h-4" />
        {intl.formatMessage(i18n.resume)}
      </Button>
    </>
  ) : null;

  return (
    <>
      <MainPanelLayout>
        <div className="flex-1 flex flex-col min-h-0 px-8">
          <SessionHeader
            onBack={onBack}
            title={session.name}
            actionButtons={!isLoading ? actionButtons : null}
          >
            <div className="flex flex-col">
              {!isLoading ? (
                <>
                  <div className="flex items-center text-text-secondary text-sm space-x-5 font-mono">
                    <span className="flex items-center">
                      <Calendar className="w-4 h-4 mr-1" />
                      {formatMessageTimestamp(messages[0]?.created)}
                    </span>
                    <span className="flex items-center">
                      <MessageSquareText className="w-4 h-4 mr-1" />
                      {session.message_count}
                    </span>
                    {session.total_tokens !== null && (
                      <span className="flex items-center">
                        <Target className="w-4 h-4 mr-1" />
                        {(session.total_tokens || 0).toLocaleString()}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center text-text-secondary text-sm mt-1 font-mono">
                    <span className="flex items-center">
                      <Folder className="w-4 h-4 mr-1" />
                      {session.working_dir}
                    </span>
                  </div>
                </>
              ) : (
                <div className="flex items-center text-text-secondary text-sm">
                  <LoaderCircle className="w-4 h-4 mr-2 animate-spin" />
                  <span>{intl.formatMessage(i18n.loadingDetails)}</span>
                </div>
              )}
            </div>
          </SessionHeader>

          <SessionMessages
            messages={messages}
            isLoading={isLoading}
            error={error}
            onRetry={onRetry}
          />
        </div>
      </MainPanelLayout>

      <Dialog open={isShareModalOpen} onOpenChange={setIsShareModalOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex justify-center items-center gap-2">
              <Share2 className="w-6 h-6 text-text-primary" />
              {intl.formatMessage(i18n.shareSessionTitle)}
            </DialogTitle>
            <DialogDescription>
              {intl.formatMessage(i18n.shareSessionDescription)}
            </DialogDescription>
          </DialogHeader>

          <div className="py-4">
            <div className="relative rounded-full border border-border-primary px-3 py-2 flex items-center bg-gray-100 dark:bg-gray-600">
              <code className="text-sm text-text-primary dark:text-text-inverse overflow-x-hidden break-all pr-8 w-full">
                {shareLink}
              </code>
              <Button
                shape="pill"
                variant="ghost"
                className="absolute right-2 top-1/2 -translate-y-1/2"
                onClick={handleCopyLink}
                disabled={isCopied}
              >
                {isCopied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                <span className="sr-only">{intl.formatMessage(i18n.copy)}</span>
              </Button>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setIsShareModalOpen(false)}>
              {intl.formatMessage(i18n.cancel)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
};

export default SessionHistoryView;
