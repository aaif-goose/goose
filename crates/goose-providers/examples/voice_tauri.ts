// Browser half of `voice_tauri.rs`.
//
// The browser owns microphone capture, audio playback, RTCPeerConnection, and
// RTCDataChannel. Rust owns credentials, provider protocol, and session state.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface StartVoiceResponse {
  sdp: string;
  sessionId?: string;
  model: string;
}

interface VoiceEvent {
  kind: Record<string, unknown> & { type: string };
  raw?: unknown;
}

let peer: RTCPeerConnection | undefined;
let dataChannel: RTCDataChannel | undefined;
let microphone: MediaStream | undefined;
let unlistenCommand: UnlistenFn | undefined;
let unlistenEvent: UnlistenFn | undefined;

export async function startVoice(initialContext: string, audio: HTMLAudioElement) {
  peer = new RTCPeerConnection();
  microphone = await navigator.mediaDevices.getUserMedia({ audio: true });
  for (const track of microphone.getTracks()) peer.addTrack(track, microphone);

  peer.ontrack = ({ streams }) => {
    audio.srcObject = streams[0];
    void audio.play();
  };

  dataChannel = peer.createDataChannel("oai-events");
  dataChannel.onmessage = ({ data }) => {
    void invoke("voice_incoming", { event: JSON.parse(data) });
  };

  unlistenCommand = await listen<Record<string, unknown>>("voice-command", ({ payload }) => {
    sendEvent(payload);
  });
  unlistenEvent = await listen<VoiceEvent>("voice-event", ({ payload }) => {
    if (payload.kind.type === "delegation_created") {
      const delegation = payload.kind.delegation as { id: string; prompt: string };
      void runDelegate(delegation.prompt).then((answer) =>
        completeDelegation(delegation.id, answer),
      );
    }
  });

  const offer = await peer.createOffer();
  await peer.setLocalDescription(offer);
  await waitForIceGathering(peer);

  const signaling = await invoke<StartVoiceResponse>("start_voice", {
    request: {
      sdp: peer.localDescription!.sdp,
      initialContext,
    },
  });
  await peer.setRemoteDescription({ type: "answer", sdp: signaling.sdp });
}

export async function appendContext(text: string) {
  await invoke("append_voice_context", { text });
}

export async function completeDelegation(delegationId: string, text: string) {
  await invoke("complete_voice_delegation", {
    request: { delegationId, text },
  });
}

export async function stopVoice() {
  await invoke("close_voice");
  peer?.close();
  microphone?.getTracks().forEach((track) => track.stop());
  unlistenCommand?.();
  unlistenEvent?.();
  peer = undefined;
  dataChannel = undefined;
  microphone = undefined;
  unlistenCommand = undefined;
  unlistenEvent = undefined;
}

function sendEvent(event: Record<string, unknown>) {
  if (dataChannel?.readyState !== "open") throw new Error("Voice data channel is not open");
  dataChannel.send(JSON.stringify(event));
}

async function waitForIceGathering(connection: RTCPeerConnection) {
  if (connection.iceGatheringState === "complete") return;
  await new Promise<void>((resolve) => {
    const listener = () => {
      if (connection.iceGatheringState === "complete") {
        connection.removeEventListener("icegatheringstatechange", listener);
        resolve();
      }
    };
    connection.addEventListener("icegatheringstatechange", listener);
  });
}

async function runDelegate(prompt: string): Promise<string> {
  return `The delegate received: ${prompt}`;
}
