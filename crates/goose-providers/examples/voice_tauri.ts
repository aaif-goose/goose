// Browser half of `voice_tauri.rs`.
//
// The browser owns microphone capture, audio playback, RTCPeerConnection, and
// RTCDataChannel. Rust owns credentials and OpenAI event encoding/decoding.

import { invoke } from "@tauri-apps/api/core";

interface StartVoiceResponse {
  sdp: string;
  sessionId?: string;
  model: string;
}

let peer: RTCPeerConnection | undefined;
let dataChannel: RTCDataChannel | undefined;
let microphone: MediaStream | undefined;

export async function startVoice(initialContext: string, audio: HTMLAudioElement) {
  peer = new RTCPeerConnection();
  microphone = await navigator.mediaDevices.getUserMedia({ audio: true });
  for (const track of microphone.getTracks()) peer.addTrack(track, microphone);

  peer.ontrack = ({ streams }) => {
    audio.srcObject = streams[0];
    void audio.play();
  };

  dataChannel = peer.createDataChannel("oai-events");
  dataChannel.onmessage = async ({ data }) => {
    const normalized = await invoke<Record<string, unknown>>("voice_incoming", {
      event: JSON.parse(data),
    });

    if (normalized.type === "delegation_created") {
      const delegation = normalized.delegation as { id: string; prompt: string };
      // Replace this with ACP, goose-agent, or an application-specific delegate.
      const answer = await runDelegate(delegation.prompt);
      await completeDelegation(delegation.id, answer);
    }
  };

  const offer = await peer.createOffer();
  await peer.setLocalDescription(offer);
  await waitForIceGathering(peer);

  const signaling = await invoke<StartVoiceResponse>("start_voice", {
    request: {
      sdp: peer.localDescription!.sdp,
      initialContext,
    },
  });

  // Rust brokers the HTTP request, so OPENAI_API_KEY never enters the WebView.
  await peer.setRemoteDescription({ type: "answer", sdp: signaling.sdp });
}

export async function appendContext(text: string) {
  const event = await invoke<Record<string, unknown>>("append_voice_context", { text });
  sendEvent(event);
}

export async function completeDelegation(delegationId: string, text: string) {
  const event = await invoke<Record<string, unknown>>("complete_voice_delegation", {
    request: { delegationId, text },
  });
  sendEvent(event);
}

export async function stopVoice() {
  if (dataChannel?.readyState === "open") {
    sendEvent(await invoke<Record<string, unknown>>("close_voice"));
  }
  peer?.close();
  microphone?.getTracks().forEach((track) => track.stop());
  peer = undefined;
  dataChannel = undefined;
  microphone = undefined;
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
