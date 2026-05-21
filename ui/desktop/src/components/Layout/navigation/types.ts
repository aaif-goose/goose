export type StreamState = 'idle' | 'loading' | 'streaming' | 'error';

export interface SessionStatus {
  streamState: StreamState;
  hasUnreadActivity: boolean;
}
