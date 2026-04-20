export type VadPhase = "idle" | "primed" | "speaking" | "trailing";

export type VadDecision = "ignore" | "append" | "append_and_flush" | "discard";

export interface VadState {
  phase: VadPhase;
  speechFrames: number;
  silenceFrames: number;
  framesInChunk: number;
}

const SPEECH_RMS_THRESHOLD = 0.018;
const SPEECH_CONFIRMATION_FRAMES = 2;
const MAX_PRIMED_SILENCE_FRAMES = 2;
const TRAILING_SILENCE_FRAMES = 6;
const MIN_SPEECH_FRAMES = 3;

export function createInitialVadState(): VadState {
  return {
    phase: "idle",
    speechFrames: 0,
    silenceFrames: 0,
    framesInChunk: 0,
  };
}

export function getFrameRms(samples: Float32Array): number {
  let sum = 0;
  for (let index = 0; index < samples.length; index += 1) {
    const value = samples[index] ?? 0;
    sum += value * value;
  }

  return Math.sqrt(sum / Math.max(samples.length, 1));
}

export function advanceVadState(
  state: VadState,
  frameRms: number,
): { decision: VadDecision; nextState: VadState } {
  const isSpeech = frameRms >= SPEECH_RMS_THRESHOLD;

  if (state.phase === "idle") {
    if (!isSpeech) {
      return { decision: "ignore" as const, nextState: state };
    }

    return {
      decision: "append" as const,
      nextState: {
        phase: "primed" as const,
        speechFrames: 1,
        silenceFrames: 0,
        framesInChunk: 1,
      },
    };
  }

  if (state.phase === "primed") {
    if (isSpeech) {
      const speechFrames = state.speechFrames + 1;
      return {
        decision: "append" as const,
        nextState: {
          phase:
            speechFrames >= SPEECH_CONFIRMATION_FRAMES ? "speaking" : "primed",
          speechFrames,
          silenceFrames: 0,
          framesInChunk: state.framesInChunk + 1,
        },
      };
    }

    const silenceFrames = state.silenceFrames + 1;
    if (silenceFrames > MAX_PRIMED_SILENCE_FRAMES) {
      return {
        decision: "discard" as const,
        nextState: createInitialVadState(),
      };
    }

    return {
      decision: "append" as const,
      nextState: {
        ...state,
        silenceFrames,
        framesInChunk: state.framesInChunk + 1,
      },
    };
  }

  if (state.phase === "speaking") {
    if (isSpeech) {
      return {
        decision: "append" as const,
        nextState: {
          phase: "speaking" as const,
          speechFrames: state.speechFrames + 1,
          silenceFrames: 0,
          framesInChunk: state.framesInChunk + 1,
        },
      };
    }

    return {
      decision: "append" as const,
      nextState: {
        phase: "trailing" as const,
        speechFrames: state.speechFrames,
        silenceFrames: 1,
        framesInChunk: state.framesInChunk + 1,
      },
    };
  }

  if (isSpeech) {
    return {
      decision: "append" as const,
      nextState: {
        phase: "speaking" as const,
        speechFrames: state.speechFrames + 1,
        silenceFrames: 0,
        framesInChunk: state.framesInChunk + 1,
      },
    };
  }

  const silenceFrames = state.silenceFrames + 1;
  if (silenceFrames < TRAILING_SILENCE_FRAMES) {
    return {
      decision: "append" as const,
      nextState: {
        ...state,
        silenceFrames,
        framesInChunk: state.framesInChunk + 1,
      },
    };
  }

  return {
    decision:
      state.speechFrames >= MIN_SPEECH_FRAMES
        ? ("append_and_flush" as const)
        : ("discard" as const),
    nextState: createInitialVadState(),
  };
}
