export class GooseClient {
  closed = Promise.resolve();

  async initialize(..._args: unknown[]): Promise<void> {}

  async listSessions(..._args: unknown[]): Promise<{ sessions: unknown[] }> {
    return { sessions: [] };
  }
}
