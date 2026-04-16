import { useCallback, useEffect, useRef, useState } from "react";
import { transcribeDictation } from "@/shared/api/dictation";
import type { DictationProvider } from "@/shared/types/dictation";
import {
  advanceVadState,
  createInitialVadState,
  getFrameRms,
} from "../lib/dictationVad";

interface UseDictationRecorderOptions {
  provider: DictationProvider | null;
  providerConfigured: boolean;
  preferredMicrophoneId: string | null;
  onError: (message: string) => void;
  onTranscription: (text: string) => void;
}

const SAMPLE_RATE = 16000;

function encodeWav(samples: Float32Array, sampleRate: number): ArrayBuffer {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const write = (offset: number, value: string) => {
    for (let index = 0; index < value.length; index += 1) {
      view.setUint8(offset + index, value.charCodeAt(index));
    }
  };

  write(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  write(8, "WAVE");
  write(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  write(36, "data");
  view.setUint32(40, samples.length * 2, true);

  let offset = 44;
  for (let index = 0; index < samples.length; index += 1) {
    const sample = Math.max(-1, Math.min(1, samples[index] ?? 0));
    view.setInt16(offset, sample < 0 ? sample * 0x8000 : sample * 0x7fff, true);
    offset += 2;
  }

  return buffer;
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => resolve(String(reader.result).split(",")[1] ?? "");
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

function toErrorMessage(error: unknown) {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  return "Voice input failed";
}

export function useDictationRecorder({
  provider,
  providerConfigured,
  preferredMicrophoneId,
  onError,
  onTranscription,
}: UseDictationRecorderOptions) {
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const audioContextRef = useRef<AudioContext | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const sourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const samplesRef = useRef<Float32Array[]>([]);
  const vadStateRef = useRef(createInitialVadState());
  const pendingTranscriptionsRef = useRef(0);
  const generationRef = useRef(0);
  // Guards against overlapping startRecording calls while getUserMedia is
  // pending (user double-clicks the mic before the first startup resolves).
  const startingRef = useRef(false);
  // Signals to an in-flight startRecording that the user has asked to stop.
  // When true, the startup path tears down any just-acquired stream instead
  // of flipping isRecording to true — otherwise the OS mic indicator would
  // stay on after the user tried to stop/send.
  const cancelStartRef = useRef(false);
  const providerRef = useRef(provider);
  providerRef.current = provider;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const onTranscriptionRef = useRef(onTranscription);
  onTranscriptionRef.current = onTranscription;

  const isEnabled = Boolean(provider && providerConfigured);

  const cleanupAudioGraph = useCallback(() => {
    processorRef.current?.disconnect();
    processorRef.current = null;
    sourceRef.current?.disconnect();
    sourceRef.current = null;
    void audioContextRef.current?.close();
    audioContextRef.current = null;
    streamRef.current?.getTracks().forEach((track) => {
      track.stop();
    });
    streamRef.current = null;
  }, []);

  const transcribeChunk = useCallback(async (samples: Float32Array) => {
    const activeProvider = providerRef.current;
    if (!activeProvider) {
      return;
    }

    const gen = generationRef.current;
    pendingTranscriptionsRef.current += 1;
    setIsTranscribing(true);

    try {
      const wavBlob = new Blob([encodeWav(samples, SAMPLE_RATE)], {
        type: "audio/wav",
      });
      const audio = await blobToBase64(wavBlob);
      const response = await transcribeDictation({
        audio,
        mimeType: "audio/wav",
        provider: activeProvider,
      });

      if (gen !== generationRef.current) {
        return;
      }

      if (response.text.trim()) {
        onTranscriptionRef.current(response.text);
      }
    } catch (error) {
      onErrorRef.current(toErrorMessage(error));
    } finally {
      pendingTranscriptionsRef.current -= 1;
      if (pendingTranscriptionsRef.current === 0) {
        setIsTranscribing(false);
      }
    }
  }, []);

  const flushPendingSamples = useCallback(() => {
    const chunks = samplesRef.current;
    if (chunks.length === 0) {
      return;
    }

    const totalSamples = chunks.reduce(
      (count, chunk) => count + chunk.length,
      0,
    );
    const merged = new Float32Array(totalSamples);
    let offset = 0;
    for (const chunk of chunks) {
      merged.set(chunk, offset);
      offset += chunk.length;
    }

    samplesRef.current = [];
    void transcribeChunk(merged);
  }, [transcribeChunk]);

  const stopRecording = useCallback(
    (options?: { flushPending?: boolean }) => {
      const flushPending = options?.flushPending ?? true;

      // Signal any in-flight startRecording to abort. If getUserMedia is
      // still pending or the audio graph hasn't been wired up yet, the
      // startup path will see this flag and clean up the just-acquired
      // stream instead of flipping isRecording to true.
      cancelStartRef.current = true;

      if (flushPending && samplesRef.current.length > 0) {
        flushPendingSamples();
      } else if (!flushPending) {
        samplesRef.current = [];
        generationRef.current += 1;
      }

      vadStateRef.current = createInitialVadState();
      cleanupAudioGraph();
      setIsRecording(false);
    },
    [cleanupAudioGraph, flushPendingSamples],
  );

  const handleFrame = useCallback(
    (samples: Float32Array) => {
      const { decision, nextState } = advanceVadState(
        vadStateRef.current,
        getFrameRms(samples),
      );
      vadStateRef.current = nextState;

      if (decision === "ignore") {
        return;
      }

      if (decision === "discard") {
        samplesRef.current = [];
        return;
      }

      samplesRef.current.push(new Float32Array(samples));

      if (decision === "append_and_flush") {
        flushPendingSamples();
      }
    },
    [flushPendingSamples],
  );

  const startRecording = useCallback(async () => {
    if (!isEnabled || !provider) {
      onError("Voice input is not configured");
      return;
    }

    // Bail if a startup is already in-flight or we're already recording.
    // Without this guard, a rapid second click (before getUserMedia resolves)
    // would kick off a parallel recorder setup and leak a MediaStream — the
    // OS mic indicator would stay on after the user thought they'd stopped.
    if (startingRef.current || isRecording) {
      return;
    }

    startingRef.current = true;
    cancelStartRef.current = false;

    try {
      const audioConstraints: MediaTrackConstraints = {
        autoGainControl: true,
        echoCancellation: true,
        noiseSuppression: true,
      };

      if (preferredMicrophoneId) {
        audioConstraints.deviceId = { exact: preferredMicrophoneId };
      }

      let stream: MediaStream;
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          audio: audioConstraints,
        });
      } catch (error) {
        if (
          preferredMicrophoneId &&
          error instanceof DOMException &&
          (error.name === "NotFoundError" ||
            error.name === "OverconstrainedError")
        ) {
          delete audioConstraints.deviceId;
          stream = await navigator.mediaDevices.getUserMedia({
            audio: audioConstraints,
          });
        } else {
          throw error;
        }
      }

      // If stopRecording was called while getUserMedia was pending (e.g.,
      // user clicked Send before the mic finished setting up), tear down
      // the freshly-acquired stream immediately and bail. Otherwise the
      // MediaStream tracks stay hot and the OS mic indicator lingers.
      if (cancelStartRef.current) {
        stream.getTracks().forEach((track) => {
          track.stop();
        });
        return;
      }

      streamRef.current = stream;
      samplesRef.current = [];
      vadStateRef.current = createInitialVadState();

      const context = new AudioContext({ sampleRate: SAMPLE_RATE });
      audioContextRef.current = context;
      await context.resume();

      // Check again after the async context.resume() — stopRecording may
      // have fired while we were awaiting.
      if (cancelStartRef.current) {
        cleanupAudioGraph();
        return;
      }

      const source = context.createMediaStreamSource(stream);
      const processor = context.createScriptProcessor(1024, 1, 1);
      const silence = context.createGain();
      silence.gain.value = 0;

      processor.onaudioprocess = (event) => {
        const channel = event.inputBuffer.getChannelData(0);
        handleFrame(new Float32Array(channel));
      };

      source.connect(processor);
      processor.connect(silence);
      silence.connect(context.destination);

      sourceRef.current = source;
      processorRef.current = processor;
      setIsRecording(true);
    } catch (error) {
      stopRecording({ flushPending: false });
      onError(toErrorMessage(error));
    } finally {
      startingRef.current = false;
    }
  }, [
    cleanupAudioGraph,
    handleFrame,
    isEnabled,
    isRecording,
    onError,
    preferredMicrophoneId,
    provider,
    stopRecording,
  ]);

  const toggleRecording = useCallback(() => {
    // If a startup is already in-flight, swallow the click. The pending
    // startRecording will either resolve into isRecording=true (and a
    // subsequent click will correctly stop) or be cancelled via
    // cancelStartRef if something else calls stopRecording in the meantime.
    if (startingRef.current) {
      return;
    }
    if (isRecording) {
      stopRecording();
    } else {
      void startRecording();
    }
  }, [isRecording, startRecording, stopRecording]);

  useEffect(
    () => () => {
      stopRecording({ flushPending: false });
    },
    [stopRecording],
  );

  useEffect(() => {
    if (!provider && isRecording) {
      stopRecording({ flushPending: false });
    }
  }, [isRecording, provider, stopRecording]);

  // Imperative check for consumers (e.g. handleSend) who need to know at
  // click time whether a startup is pending. Uses a function rather than a
  // state value because startingRef is a ref (no render on change) and we
  // only need the answer when the consumer is deciding what to do *now*.
  const isStarting = useCallback(() => startingRef.current, []);

  return {
    isEnabled,
    isRecording,
    isTranscribing,
    isStarting,
    startRecording,
    stopRecording,
    toggleRecording,
  };
}
