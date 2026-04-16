import {
  ClientSideConnection,
  type Client,
  type Stream,
  type InitializeRequest,
  type InitializeResponse,
  type NewSessionRequest,
  type NewSessionResponse,
  type LoadSessionRequest,
  type LoadSessionResponse,
  type PromptRequest,
  type PromptResponse,
  type CancelNotification,
  type AuthenticateRequest,
  type AuthenticateResponse,
  type SetSessionModeRequest,
  type SetSessionModeResponse,
  type SetSessionConfigOptionRequest,
  type SetSessionConfigOptionResponse,
  type ForkSessionRequest,
  type ForkSessionResponse,
  type ListSessionsRequest,
  type ListSessionsResponse,
  type ResumeSessionRequest,
  type ResumeSessionResponse,
  type SetSessionModelRequest,
  type SetSessionModelResponse,
} from "@agentclientprotocol/sdk";
import { GooseExtClient } from "./generated/client.gen.js";
import { createHttpStream } from "./http-stream.js";

export class GooseClient {
  private conn: ClientSideConnection;
  private ext: GooseExtClient;

  constructor(toClient: () => Client, streamOrUrl: Stream | string) {
    const stream =
      typeof streamOrUrl === "string"
        ? createHttpStream(streamOrUrl)
        : streamOrUrl;
    this.conn = new ClientSideConnection(toClient, stream);
    this.ext = new GooseExtClient(this.conn);
  }

  get signal(): AbortSignal {
    return this.conn.signal;
  }

  get closed(): Promise<void> {
    return this.conn.closed;
  }

  initialize(params: InitializeRequest): Promise<InitializeResponse> {
    return this.conn.initialize(params);
  }

  newSession(params: NewSessionRequest): Promise<NewSessionResponse> {
    return this.conn.newSession(params);
  }

  loadSession(params: LoadSessionRequest): Promise<LoadSessionResponse> {
    return this.conn.loadSession(params);
  }

  prompt(params: PromptRequest): Promise<PromptResponse> {
    return this.conn.prompt(params);
  }

  cancel(params: CancelNotification): Promise<void> {
    return this.conn.cancel(params);
  }

  authenticate(params: AuthenticateRequest): Promise<AuthenticateResponse> {
    return this.conn.authenticate(params);
  }

  setSessionMode(
    params: SetSessionModeRequest,
  ): Promise<SetSessionModeResponse> {
    return this.conn.setSessionMode(params);
  }

  setSessionConfigOption(
    params: SetSessionConfigOptionRequest,
  ): Promise<SetSessionConfigOptionResponse> {
    return this.conn.setSessionConfigOption(params);
  }

  unstable_forkSession(
    params: ForkSessionRequest,
  ): Promise<ForkSessionResponse> {
    return this.conn.unstable_forkSession(params);
  }

  unstable_listSessions(
    params: ListSessionsRequest,
  ): Promise<ListSessionsResponse> {
    return this.conn.unstable_listSessions(params);
  }

  unstable_resumeSession(
    params: ResumeSessionRequest,
  ): Promise<ResumeSessionResponse> {
    return this.conn.unstable_resumeSession(params);
  }

  unstable_setSessionModel(
    params: SetSessionModelRequest,
  ): Promise<SetSessionModelResponse> {
    return this.conn.unstable_setSessionModel(params);
  }

  extMethod(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    return this.conn.extMethod(method, params);
  }

  get goose(): GooseExtClient {
    return this.ext;
  }
}
