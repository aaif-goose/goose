import { useCallback, useEffect, useRef, useState } from 'react';
import { isAcpRecovering, subscribeToAcpRecovery } from '../acp/acpConnection';
import { acpStartLiveVoice, acpStopLiveVoice } from '../acp/liveVoice';
import {
  subscribeToLiveVoiceCallEnded,
  type LiveVoiceCallEndedNotification,
} from '../acp/liveVoiceNotifications';
import { LiveVoiceMediaSession } from './LiveVoiceMediaSession';

export type LiveVoicePhase = 'idle' | 'connecting' | 'live' | 'stopping' | 'error';

export function isLiveVoiceActive(phase: LiveVoicePhase): boolean {
  return phase === 'connecting' || phase === 'live' || phase === 'stopping';
}

export interface LiveVoiceController {
  phase: LiveVoicePhase;
  muted: boolean;
  start: (initialCommentary?: string) => Promise<void>;
  stop: () => Promise<void>;
  toggleMute: () => void;
}

interface LiveVoiceCall {
  sessionId: string;
  callId?: string;
  remoteStartPending: boolean;
  media: LiveVoiceMediaSession;
  mediaReady: boolean;
  invalidated: boolean;
  acpConnectionLost: boolean;
  pendingOutcomesByCallId: Map<string, LiveVoiceCallEndedNotification['update']['outcome']>;
}

async function stopRemoteCall(call: LiveVoiceCall): Promise<'stopped' | 'failed'> {
  if (!call.callId || call.acpConnectionLost) return 'stopped';
  try {
    await acpStopLiveVoice(call.sessionId, call.callId);
    return 'stopped';
  } catch {
    return 'failed';
  }
}

export function useLiveVoice(sessionId: string, isSessionActive: boolean): LiveVoiceController {
  const [phase, setPhase] = useState<LiveVoicePhase>('idle');
  const [muted, setMuted] = useState(false);
  const mutedRef = useRef(false);
  const callRef = useRef<LiveVoiceCall | null>(null);

  const invalidateCallAndReleaseMedia = useCallback((call: LiveVoiceCall) => {
    if (call.invalidated) return;
    call.invalidated = true;
    call.media.teardown();
    mutedRef.current = false;
  }, []);

  const finishCurrentCall = useCallback(
    (call: LiveVoiceCall, outcome: LiveVoiceCallEndedNotification['update']['outcome']) => {
      if (callRef.current !== call) return false;

      callRef.current = null;
      invalidateCallAndReleaseMedia(call);
      setMuted(false);
      setPhase(outcome === 'failed' ? 'error' : 'idle');
      return true;
    },
    [invalidateCallAndReleaseMedia]
  );

  useEffect(() => {
    setPhase('idle');
    mutedRef.current = false;
    setMuted(false);
    if (!isSessionActive) return;

    return () => {
      const call = callRef.current;
      if (!call || call.sessionId !== sessionId) return;

      callRef.current = null;
      invalidateCallAndReleaseMedia(call);
      void stopRemoteCall(call);
    };
  }, [invalidateCallAndReleaseMedia, isSessionActive, sessionId]);

  useEffect(() => {
    return subscribeToLiveVoiceCallEnded((notification) => {
      const call = callRef.current;
      if (!call || call.sessionId !== notification.sessionId) return;

      if (!call.callId) {
        call.pendingOutcomesByCallId.set(notification.update.callId, notification.update.outcome);
        return;
      }
      if (!call.invalidated && call.callId === notification.update.callId) {
        finishCurrentCall(call, notification.update.outcome);
      }
    });
  }, [finishCurrentCall]);

  useEffect(() => {
    return subscribeToAcpRecovery((recovering) => {
      if (!recovering) return;

      const call = callRef.current;
      if (call?.sessionId === sessionId) {
        call.acpConnectionLost = true;
        finishCurrentCall(call, 'stopped');
      }
    });
  }, [finishCurrentCall, sessionId]);

  const start = useCallback(
    async (initialCommentary?: string) => {
      if (!isSessionActive || callRef.current || isAcpRecovering()) return;

      mutedRef.current = false;
      setMuted(false);
      setPhase('connecting');
      let call: LiveVoiceCall;
      const media = new LiveVoiceMediaSession(() => {
        if (!finishCurrentCall(call, 'failed')) return;
        void stopRemoteCall(call);
      });
      call = {
        sessionId,
        remoteStartPending: false,
        media,
        mediaReady: false,
        invalidated: false,
        acpConnectionLost: false,
        pendingOutcomesByCallId: new Map(),
      };
      callRef.current = call;
      const isCurrent = () => callRef.current === call && !call.invalidated;

      try {
        const offerSdp = await call.media.createOffer();
        if (!isCurrent()) return;

        call.remoteStartPending = true;
        const response = await acpStartLiveVoice(sessionId, offerSdp);
        call.remoteStartPending = false;
        call.callId = response.callId;
        const pendingOutcome = call.pendingOutcomesByCallId.get(call.callId);
        call.pendingOutcomesByCallId.clear();
        if (pendingOutcome) {
          finishCurrentCall(call, pendingOutcome);
          return;
        }
        if (!isCurrent()) {
          finishCurrentCall(call, await stopRemoteCall(call));
          return;
        }

        await call.media.applyAnswer(response.answerSdp);
        if (!isCurrent()) return;

        call.mediaReady = true;
        call.media.setMuted(mutedRef.current);
        setPhase('live');
        if (initialCommentary) {
          call.media.sendCommentary(initialCommentary);
        }
      } catch {
        call.remoteStartPending = false;
        const outcome = call.invalidated ? 'stopped' : 'failed';
        if (finishCurrentCall(call, outcome) && outcome === 'failed') {
          void stopRemoteCall(call);
        }
      }
    },
    [finishCurrentCall, isSessionActive, sessionId]
  );

  const toggleMute = useCallback(() => {
    const call = callRef.current;
    if (!call || call.invalidated || !call.mediaReady) return;

    mutedRef.current = !mutedRef.current;
    call.media.setMuted(mutedRef.current);
    setMuted(mutedRef.current);
  }, []);

  const stop = useCallback(async () => {
    const call = callRef.current;
    if (!call || call.invalidated) return;

    invalidateCallAndReleaseMedia(call);
    setMuted(false);
    if (!call.callId) {
      if (call.remoteStartPending) {
        setPhase('stopping');
        return;
      }
      callRef.current = null;
      setPhase('idle');
      return;
    }

    setPhase('stopping');
    finishCurrentCall(call, await stopRemoteCall(call));
  }, [finishCurrentCall, invalidateCallAndReleaseMedia]);

  return { phase, muted, start, stop, toggleMute };
}
