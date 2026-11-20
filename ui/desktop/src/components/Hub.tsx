/**
 * Hub Component
 *
 * The empty-chat landing screen. Visually it's "Pair with no messages yet" —
 * a large time + greeting above a centered, narrower ChatInput. Enter
 * navigates to /pair immediately; Pair creates the session in the background.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { useLocation } from 'react-router';
import { defineMessages, useIntl } from '../i18n';
import { AppEvents } from '../constants/events';
import ChatInput from './ChatInput';
import { ChatInputCard } from './ChatInputCard';
import { ChatState } from '../types/chatState';
import 'react-toastify/dist/ReactToastify.css';
import { rememberHubSubmit, takeHubSubmit, View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import { getEffectiveWorkingDir, getInitialWorkingDir } from '../utils/workingDir';
import { createSession } from '../sessions';
import { UserInput } from '../types/message';
import {
  createNextChatExtensionDraft,
  selectNextChatExtensions,
  type NextChatExtensionDraft,
} from '../utils/nextChatExtensions';
import { formatAcpError } from '../acp/errors';
import { toastError } from '../toasts';
import { formatClockDisplay } from '../utils/timeUtils';
import { acpGetLiveVoiceAvailability } from '../acp/liveVoice';
import type { LiveVoiceAvailabilityResponse_unstable } from '@aaif/goose-acp-client';
import { subscribeToAcpRecovery } from '../acp/acpConnection';
import type { LiveVoiceController } from '../liveVoice/useLiveVoice';

const i18n = defineMessages({
  goodMorning: { id: 'hub.goodMorning', defaultMessage: 'Good morning' },
  goodAfternoon: { id: 'hub.goodAfternoon', defaultMessage: 'Good afternoon' },
  goodEvening: { id: 'hub.goodEvening', defaultMessage: 'Good evening' },
});

function useClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const interval = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(interval);
  }, []);

  return formatClockDisplay(now);
}

export default function Hub({
  setView,
  draftRef,
  liveVoice,
}: {
  setView: (view: View, viewOptions?: ViewOptions) => void;
  /** Unsent input of this screen, kept above the route outlet across the unmount. */
  draftRef: RefObject<string>;
  liveVoice: LiveVoiceController;
}) {
  const intl = useIntl();
  const { state } = useLocation();
  const initialWorkingDir = (state as { workingDir?: string } | null)?.workingDir;
  const { extensionsList } = useConfig();
  const pendingSubmit = takeHubSubmit(draftRef);
  const [workingDir, setWorkingDir] = useState(
    pendingSubmit?.workingDir ?? initialWorkingDir ?? getInitialWorkingDir()
  );
  const userSelectedWorkingDirRef = useRef(pendingSubmit?.workingDir !== undefined);
  const [liveVoiceAvailability, setLiveVoiceAvailability] =
    useState<LiveVoiceAvailabilityResponse_unstable | null>(null);
  const [nextChatExtensionDraft, setNextChatExtensionDraft] =
    useState<NextChatExtensionDraft | null>(pendingSubmit?.extensionDraft ?? null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const { time, meridiem, hour } = useClock();

  // Re-resolve the working dir on mount: GOOSE_WORKING_DIR is fixed at window
  // creation, so a configured remote directory may have changed since then.
  useEffect(() => {
    let active = true;
    void getEffectiveWorkingDir(initialWorkingDir).then((dir) => {
      if (active && !userSelectedWorkingDirRef.current) setWorkingDir(dir);
    });
    return () => {
      active = false;
    };
  }, [initialWorkingDir]);

  useEffect(() => {
    let requestGeneration = 0;
    const requestAvailability = async () => {
      const generation = ++requestGeneration;
      try {
        const availability = await acpGetLiveVoiceAvailability();
        if (generation === requestGeneration) {
          setLiveVoiceAvailability(availability);
        }
      } catch {
        if (generation === requestGeneration) {
          setLiveVoiceAvailability(null);
        }
      }
    };

    void requestAvailability();
    const unsubscribe = subscribeToAcpRecovery((recovering) => {
      if (!recovering) {
        void requestAvailability();
      }
    });

    return () => {
      requestGeneration += 1;
      unsubscribe();
    };
  }, []);

  const greeting = useMemo(() => {
    if (hour < 12) return intl.formatMessage(i18n.goodMorning);
    if (hour < 18) return intl.formatMessage(i18n.goodAfternoon);
    return intl.formatMessage(i18n.goodEvening);
  }, [intl, hour]);

  const draftForMenu = useMemo(
    () => nextChatExtensionDraft ?? createNextChatExtensionDraft(extensionsList),
    [extensionsList, nextChatExtensionDraft]
  );

  // rAF is more reliable than autoFocus across async render boundaries.
  useEffect(() => {
    const frameId = requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => cancelAnimationFrame(frameId);
  }, []);

  const handleNextChatExtensionDraftChange = useCallback((draft: NextChatExtensionDraft) => {
    setNextChatExtensionDraft(draft);
  }, []);

  const handleWorkingDirChange = useCallback((dir: string) => {
    userSelectedWorkingDirRef.current = true;
    setWorkingDir(dir);
  }, []);

  const handleSubmit = (input: UserInput) => {
    const { msg: userMessage, images } = input;
    if (!(images.length > 0 || userMessage.trim())) return;

    // ChatInput only clears this draft when sessionId !== null, so the text
    // stays put until Pair clears it after session/new succeeds. Images and
    // Hub options are not in that string; Pair puts them back if creation fails.
    rememberHubSubmit(draftRef, {
      text: userMessage,
      images,
      workingDir: userSelectedWorkingDirRef.current ? workingDir : undefined,
      extensionDraft: nextChatExtensionDraft,
    });
    const sessionOptions = nextChatExtensionDraft
      ? { extensionConfigs: selectNextChatExtensions(extensionsList, nextChatExtensionDraft) }
      : {};

    setView('pair', {
      disableAnimation: true,
      fromHub: true,
      initialMessage: { msg: userMessage, images },
      // Only an explicit pick is final. Pair re-resolves an automatic directory,
      // including one inherited from the previous chat, against the backend.
      ...(userSelectedWorkingDirRef.current ? { workingDir } : {}),
      ...sessionOptions,
    });
  };

  const handleStartLiveVoice = async () => {
    if (liveVoice.activeSessionId) {
      setView('pair', { resumeSessionId: liveVoice.activeSessionId });
      return;
    }

    const sessionOptions = nextChatExtensionDraft
      ? { extensionConfigs: selectNextChatExtensions(extensionsList, nextChatExtensionDraft) }
      : { allExtensions: extensionsList };
    const dir = userSelectedWorkingDirRef.current
      ? workingDir
      : await getEffectiveWorkingDir(initialWorkingDir);

    try {
      const session = await createSession(dir, sessionOptions);
      setNextChatExtensionDraft(null);
      window.dispatchEvent(new CustomEvent(AppEvents.SESSION_CREATED));
      window.dispatchEvent(
        new CustomEvent(AppEvents.ADD_ACTIVE_SESSION, {
          detail: { sessionId: session.id },
        })
      );
      setView('pair', {
        disableAnimation: true,
        resumeSessionId: session.id,
        startLiveVoice: true,
      });
    } catch (error) {
      console.error('Failed to create session:', error);
      toastError({ title: "Couldn't start chat", msg: formatAcpError(error) });
    }
  };

  return (
    <div className="flex flex-col h-full min-h-0 items-center justify-center px-6 relative">
      <div className="w-full max-w-3xl">
        <div className="flex items-baseline gap-2 mb-1">
          <span className="text-6xl font-light text-text-primary tracking-tight tabular-nums">
            {time}
          </span>
          {meridiem ? (
            <span className="text-2xl font-light text-text-secondary">{meridiem}</span>
          ) : null}
        </div>
        <p className="text-xl text-text-secondary mb-6">{greeting}</p>

        <ChatInputCard>
          <ChatInput
            sessionId={null}
            draftRef={draftRef}
            handleSubmit={handleSubmit}
            chatState={ChatState.Idle}
            hasActiveRun={false}
            onStop={() => {}}
            initialValue=""
            initialImages={pendingSubmit?.images}
            setView={setView}
            totalTokens={0}
            accumulatedInputTokens={0}
            accumulatedOutputTokens={0}
            droppedFiles={[]}
            onFilesProcessed={() => {}}
            messages={[]}
            disableAnimation={false}
            workingDir={workingDir}
            onWorkingDirChange={handleWorkingDirChange}
            inputRef={inputRef}
            nextChatExtensionDraft={draftForMenu}
            onNextChatExtensionDraftChange={handleNextChatExtensionDraftChange}
            liveVoice={{
              availability: liveVoiceAvailability,
              phase: 'idle',
              muted: false,
              activeInAnotherSession: liveVoice.activeSessionId !== null,
              start: handleStartLiveVoice,
              stop: liveVoice.stop,
              toggleMute: liveVoice.toggleMute,
            }}
          />
        </ChatInputCard>
      </div>

    </div>
  );
}
