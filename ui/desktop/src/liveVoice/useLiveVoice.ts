import { useCallback, useEffect, useRef, useState } from 'react';
import { isAcpRecovering, subscribeToAcpRecovery } from '../acp/acpConnection';
import { acpStartLiveVoice, acpStopLiveVoice } from '../acp/liveVoice';
import {
  subscribeToLiveVoiceInteractionEnded,
  type LiveVoiceInteractionEndedNotification,
} from '../acp/liveVoiceNotifications';
import { LiveVoiceMediaSession } from './LiveVoiceMediaSession';

export type LiveVoicePhase = 'idle' | 'connecting' | 'live' | 'stopping' | 'error';

export function isLiveVoiceActive(phase: LiveVoicePhase): boolean {
  return phase === 'connecting' || phase === 'live' || phase === 'stopping';
}

export interface LiveVoiceController {
  activeSessionId: string | null;
  liveVoiceSessionId: string | null;
  phase: LiveVoicePhase;
  muted: boolean;
  start: (sessionId: string, initialCommentary?: string) => Promise<void>;
  stop: () => Promise<void>;
  toggleMute: () => void;
}

interface LiveVoiceInteraction {
  sessionId: string;
  interactionId?: string;
  remoteStartPending: boolean;
  media: LiveVoiceMediaSession;
  mediaReady: boolean;
  invalidated: boolean;
  acpConnectionLost: boolean;
  pendingOutcomesByInteractionId: Map<
    string,
    LiveVoiceInteractionEndedNotification['update']['outcome']
  >;
}

async function stopRemoteInteraction(
  interaction: LiveVoiceInteraction
): Promise<'stopped' | 'failed'> {
  if (!interaction.interactionId || interaction.acpConnectionLost) return 'stopped';
  try {
    await acpStopLiveVoice(interaction.sessionId, interaction.interactionId);
    return 'stopped';
  } catch {
    return 'failed';
  }
}

export function useLiveVoice(): LiveVoiceController {
  const [liveVoiceSessionId, setLiveVoiceSessionId] = useState<string | null>(null);
  const [phase, setPhase] = useState<LiveVoicePhase>('idle');
  const [muted, setMuted] = useState(false);
  const mutedRef = useRef(false);
  const interactionRef = useRef<LiveVoiceInteraction | null>(null);

  const invalidateInteractionAndReleaseMedia = useCallback((interaction: LiveVoiceInteraction) => {
    if (interaction.invalidated) return;
    interaction.invalidated = true;
    interaction.media.teardown();
    mutedRef.current = false;
  }, []);

  const finishCurrentInteraction = useCallback(
    (
      interaction: LiveVoiceInteraction,
      outcome: LiveVoiceInteractionEndedNotification['update']['outcome']
    ) => {
      if (interactionRef.current !== interaction) return false;

      interactionRef.current = null;
      invalidateInteractionAndReleaseMedia(interaction);
      if (outcome !== 'failed') {
        setLiveVoiceSessionId(null);
      }
      setMuted(false);
      setPhase(outcome === 'failed' ? 'error' : 'idle');
      return true;
    },
    [invalidateInteractionAndReleaseMedia]
  );

  const failCurrentInteraction = useCallback(
    async (interaction: LiveVoiceInteraction) => {
      if (interactionRef.current !== interaction || interaction.invalidated) return;

      invalidateInteractionAndReleaseMedia(interaction);
      setMuted(false);
      if (interaction.interactionId) {
        setPhase('stopping');
        await stopRemoteInteraction(interaction);
      }
      finishCurrentInteraction(interaction, 'failed');
    },
    [finishCurrentInteraction, invalidateInteractionAndReleaseMedia]
  );

  useEffect(() => {
    return () => {
      const interaction = interactionRef.current;
      if (!interaction) return;

      interactionRef.current = null;
      invalidateInteractionAndReleaseMedia(interaction);
      void stopRemoteInteraction(interaction);
    };
  }, [invalidateInteractionAndReleaseMedia]);

  useEffect(() => {
    return subscribeToLiveVoiceInteractionEnded((notification) => {
      const interaction = interactionRef.current;
      if (!interaction || interaction.sessionId !== notification.sessionId) return;

      if (!interaction.interactionId) {
        interaction.pendingOutcomesByInteractionId.set(
          notification.update.interactionId,
          notification.update.outcome
        );
        return;
      }
      if (
        !interaction.invalidated &&
        interaction.interactionId === notification.update.interactionId
      ) {
        finishCurrentInteraction(interaction, notification.update.outcome);
      }
    });
  }, [finishCurrentInteraction]);

  useEffect(() => {
    return subscribeToAcpRecovery((recovering) => {
      if (!recovering) return;

      const interaction = interactionRef.current;
      if (interaction) {
        interaction.acpConnectionLost = true;
        finishCurrentInteraction(interaction, 'stopped');
      }
    });
  }, [finishCurrentInteraction]);

  const start = useCallback(
    async (sessionId: string, initialCommentary?: string) => {
      if (interactionRef.current || isAcpRecovering()) return;

      setLiveVoiceSessionId(sessionId);
      mutedRef.current = false;
      setMuted(false);
      setPhase('connecting');
      let interaction: LiveVoiceInteraction;
      const media = new LiveVoiceMediaSession(() => {
        void failCurrentInteraction(interaction);
      });
      interaction = {
        sessionId,
        remoteStartPending: false,
        media,
        mediaReady: false,
        invalidated: false,
        acpConnectionLost: false,
        pendingOutcomesByInteractionId: new Map(),
      };
      interactionRef.current = interaction;
      const isCurrent = () => interactionRef.current === interaction && !interaction.invalidated;

      try {
        const offerSdp = await interaction.media.createOffer();
        if (!isCurrent()) return;

        interaction.remoteStartPending = true;
        const response = await acpStartLiveVoice(sessionId, offerSdp);
        interaction.remoteStartPending = false;
        interaction.interactionId = response.interactionId;
        const pendingOutcome = interaction.pendingOutcomesByInteractionId.get(
          interaction.interactionId
        );
        interaction.pendingOutcomesByInteractionId.clear();
        if (pendingOutcome) {
          finishCurrentInteraction(interaction, pendingOutcome);
          return;
        }
        if (!isCurrent()) {
          finishCurrentInteraction(interaction, await stopRemoteInteraction(interaction));
          return;
        }

        await interaction.media.applyAnswer(response.answerSdp);
        if (!isCurrent()) return;

        interaction.mediaReady = true;
        interaction.media.setMuted(mutedRef.current);
        setPhase('live');
        if (initialCommentary) {
          interaction.media.sendCommentary(initialCommentary);
        }
      } catch {
        interaction.remoteStartPending = false;
        if (interaction.invalidated) {
          if (!interaction.interactionId) finishCurrentInteraction(interaction, 'stopped');
          return;
        }
        await failCurrentInteraction(interaction);
      }
    },
    [failCurrentInteraction, finishCurrentInteraction]
  );

  const toggleMute = useCallback(() => {
    const interaction = interactionRef.current;
    if (!interaction || interaction.invalidated || !interaction.mediaReady) return;

    mutedRef.current = !mutedRef.current;
    interaction.media.setMuted(mutedRef.current);
    setMuted(mutedRef.current);
  }, []);

  const stop = useCallback(async () => {
    const interaction = interactionRef.current;
    if (!interaction || interaction.invalidated) return;

    invalidateInteractionAndReleaseMedia(interaction);
    setMuted(false);
    if (!interaction.interactionId) {
      if (interaction.remoteStartPending) {
        setPhase('stopping');
        return;
      }
      interactionRef.current = null;
      setLiveVoiceSessionId(null);
      setPhase('idle');
      return;
    }

    setPhase('stopping');
    finishCurrentInteraction(interaction, await stopRemoteInteraction(interaction));
  }, [finishCurrentInteraction, invalidateInteractionAndReleaseMedia]);

  const activeSessionId = isLiveVoiceActive(phase) ? liveVoiceSessionId : null;
  return { activeSessionId, liveVoiceSessionId, phase, muted, start, stop, toggleMute };
}
