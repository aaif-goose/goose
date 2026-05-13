import { useEffect, useState, useCallback } from 'react';
import { MessageSquarePlus, Inbox, LoaderCircle, X, Check, Share2, Copy } from 'lucide-react';
import { Button } from './ui/button';
import { getChannelInfo, getSuggestions, sendSuggestion, shareSessionNostr } from '../api/sdk.gen';
import { toast } from 'react-toastify';
import type { SuggestionEvent } from '../hooks/useSessionEvents';

interface Suggestion {
  text: string;
  senderName?: string | null;
  eventId: string;
  timestamp: number;
}

interface SuggestionBannerProps {
  sessionId: string;
  onAcceptSuggestion: (text: string) => void;
  setSuggestionHandler: (handler: ((suggestion: SuggestionEvent) => void) | null) => void;
}

const SENDER_NAME_KEY = 'goose_suggestion_sender_name';

function getSavedSenderName(): string {
  try {
    return localStorage.getItem(SENDER_NAME_KEY) || '';
  } catch {
    return '';
  }
}

function saveSenderName(name: string) {
  try {
    localStorage.setItem(SENDER_NAME_KEY, name);
  } catch {
    // ignore
  }
}

export default function SuggestionBanner({ sessionId, onAcceptSuggestion, setSuggestionHandler }: SuggestionBannerProps) {
  const [role, setRole] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [showSendInput, setShowSendInput] = useState(false);
  const [suggestionText, setSuggestionText] = useState('');
  const [senderName, setSenderName] = useState(getSavedSenderName);
  const [isSending, setIsSending] = useState(false);
  const [isSharing, setIsSharing] = useState(false);
  const [shareLink, setShareLink] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getChannelInfo({ path: { session_id: sessionId }, throwOnError: false })
      .then((resp) => {
        if (!cancelled) {
          if (resp.data) {
            setRole(resp.data.role);
          }
          setChecked(true);
        }
      })
      .catch(() => {
        if (!cancelled) setChecked(true);
      });
    return () => { cancelled = true; };
  }, [sessionId]);

  // For owners: fetch existing suggestions once, then listen for new ones via SSE
  useEffect(() => {
    if (role !== 'owner') return;

    // One-time fetch for suggestions that arrived while session was closed
    setIsLoading(true);
    getSuggestions({ path: { session_id: sessionId }, throwOnError: false })
      .then((resp) => {
        if (resp.data) {
          setSuggestions(resp.data);
        }
      })
      .catch(() => {})
      .finally(() => setIsLoading(false));

    // Listen for real-time suggestions via SSE
    setSuggestionHandler((suggestion: SuggestionEvent) => {
      setSuggestions((prev) => {
        // Deduplicate by eventId
        if (prev.some((s) => s.eventId === suggestion.eventId)) return prev;
        return [...prev, suggestion];
      });
    });

    return () => setSuggestionHandler(null);
  }, [role, sessionId, setSuggestionHandler]);

  const handleSend = useCallback(async () => {
    if (!suggestionText.trim()) return;
    setIsSending(true);
    if (senderName.trim()) {
      saveSenderName(senderName.trim());
    }
    try {
      await sendSuggestion({
        path: { session_id: sessionId },
        body: {
          text: suggestionText.trim(),
          senderName: senderName.trim() || undefined,
        },
        throwOnError: true,
      });
      setSuggestionText('');
      setShowSendInput(false);
      toast.success('Suggestion sent');
    } catch {
      toast.error('Failed to send suggestion');
    } finally {
      setIsSending(false);
    }
  }, [sessionId, suggestionText, senderName]);

  const handleShare = useCallback(async () => {
    setIsSharing(true);
    try {
      const resp = await shareSessionNostr({
        path: { session_id: sessionId },
        body: {},
        throwOnError: true,
      });
      if (resp.data?.deeplink) {
        setShareLink(resp.data.deeplink);
        setRole('owner');
        await navigator.clipboard.writeText(resp.data.deeplink);
        toast.success('Share link copied to clipboard');
      }
    } catch {
      toast.error('Failed to share session');
    } finally {
      setIsSharing(false);
    }
  }, [sessionId]);

  const handleAccept = useCallback((suggestion: Suggestion) => {
    setSuggestions((prev) => prev.filter((s) => s.eventId !== suggestion.eventId));
    onAcceptSuggestion(suggestion.text);
  }, [onAcceptSuggestion]);

  const handleDismiss = useCallback((eventId: string) => {
    setSuggestions((prev) => prev.filter((s) => s.eventId !== eventId));
  }, []);

  if (!checked) return null;

  // No channel yet — offer to share
  if (!role) {
    return (
      <div className="mx-4 mb-2">
        {shareLink ? (
          <div className="rounded-lg border border-border-primary bg-background-secondary p-3 space-y-2">
            <p className="text-sm text-text-primary">Session shared! Link copied to clipboard.</p>
            <div className="flex items-center gap-2">
              <code className="flex-1 text-xs bg-background-primary p-2 rounded truncate">{shareLink}</code>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  navigator.clipboard.writeText(shareLink);
                  toast.success('Copied');
                }}
              >
                <Copy className="w-3 h-3" />
              </Button>
            </div>
          </div>
        ) : (
          <button
            onClick={handleShare}
            disabled={isSharing}
            className="flex items-center gap-2 text-sm text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50"
          >
            {isSharing ? (
              <LoaderCircle className="w-4 h-4 animate-spin" />
            ) : (
              <Share2 className="w-4 h-4" />
            )}
            Share this session for collaboration
          </button>
        )}
      </div>
    );
  }

  // Participant: send suggestion
  if (role === 'participant') {
    if (showSendInput) {
      return (
        <div className="mx-4 mb-2 rounded-lg border border-border-primary bg-background-secondary p-3 space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium flex items-center gap-2">
              <MessageSquarePlus className="w-4 h-4" />
              Send suggestion to sharer
            </span>
            <button onClick={() => setShowSendInput(false)} className="p-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded">
              <X className="w-4 h-4 text-text-secondary" />
            </button>
          </div>
          <input
            type="text"
            className="w-full rounded border border-border-primary bg-background-primary p-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="Your name (optional)"
            value={senderName}
            onChange={(e) => setSenderName(e.target.value)}
          />
          <textarea
            className="w-full min-h-[60px] rounded border border-border-primary bg-background-primary p-2 text-sm resize-y focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="Type your suggestion..."
            value={suggestionText}
            onChange={(e) => setSuggestionText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                handleSend();
              }
            }}
          />
          <div className="flex justify-end">
            <Button size="sm" onClick={handleSend} disabled={!suggestionText.trim() || isSending}>
              {isSending ? <LoaderCircle className="w-4 h-4 animate-spin mr-1" /> : null}
              Send
            </Button>
          </div>
        </div>
      );
    }

    return (
      <div className="mx-4 mb-2">
        <button
          onClick={() => setShowSendInput(true)}
          className="flex items-center gap-2 text-sm text-text-secondary hover:text-text-primary transition-colors"
        >
          <MessageSquarePlus className="w-4 h-4" />
          Send a suggestion to the person who shared this session
        </button>
      </div>
    );
  }

  // Owner: show incoming suggestions
  if (role === 'owner') {
    if (isLoading && suggestions.length === 0) return null;
    if (suggestions.length === 0) return null;

    return (
      <div className="mx-4 mb-2 space-y-2">
        {suggestions.map((suggestion) => (
          <div
            key={suggestion.eventId}
            className="rounded-lg border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/30 p-3 space-y-2"
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex-1">
                <div className="flex items-center gap-2 mb-1">
                  <Inbox className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                  <span className="text-xs font-medium text-blue-600 dark:text-blue-400">
                    {suggestion.senderName
                      ? `Suggestion from ${suggestion.senderName}`
                      : 'Suggestion received'}
                  </span>
                  <span className="text-xs text-text-secondary">
                    {new Date(suggestion.timestamp * 1000).toLocaleString()}
                  </span>
                </div>
                <p className="text-sm text-text-primary whitespace-pre-wrap">{suggestion.text}</p>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => handleDismiss(suggestion.eventId)}
              >
                <X className="w-3 h-3 mr-1" />
                Dismiss
              </Button>
              <Button
                size="sm"
                onClick={() => handleAccept(suggestion)}
              >
                <Check className="w-3 h-3 mr-1" />
                Accept & run
              </Button>
            </div>
          </div>
        ))}
      </div>
    );
  }

  return null;
}
