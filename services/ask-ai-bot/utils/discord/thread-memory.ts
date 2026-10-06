import { mkdir, readFile, rename, writeFile } from "fs/promises";
import path from "path";
import { redactSecrets } from "./message-content";

export class ThreadMemory {
  constructor(
    private directory = process.env.THREAD_STATE_PATH ||
      path.join(process.cwd(), ".data", "threads"),
  ) {}

  async read(threadId: string): Promise<string> {
    try {
      return await readFile(this.filePath(threadId), "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return "";
      throw error;
    }
  }

  async write(threadId: string, memory: string): Promise<void> {
    await mkdir(this.directory, { recursive: true });
    const destination = this.filePath(threadId);
    const temporary = `${destination}.${crypto.randomUUID()}.tmp`;
    await writeFile(temporary, redactSecrets(memory).slice(0, 2000), {
      mode: 0o600,
    });
    await rename(temporary, destination);
  }

  private filePath(threadId: string): string {
    if (!/^\d+$/.test(threadId)) throw new Error("Invalid Discord thread ID");
    return path.join(this.directory, `${threadId}.txt`);
  }
}

const pending = new Map<string, Promise<unknown>>();

export async function inThreadOrder<T>(
  threadId: string,
  action: () => Promise<T>,
): Promise<T> {
  const previous = pending.get(threadId) ?? Promise.resolve();
  const next = previous.catch(() => {}).then(action);
  pending.set(threadId, next);
  try {
    return await next;
  } finally {
    if (pending.get(threadId) === next) pending.delete(threadId);
  }
}
