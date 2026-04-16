import net from "node:net";

const PORT = Number(process.env.APP_TEST_DRIVER_PORT) || 9999;

interface TestDriverCommand {
  action: string;
  selector?: string;
  value?: string;
  timeout?: number;
}

interface TestDriverResult {
  success: boolean;
  data?: string;
  error?: string;
}

export interface TestDriver {
  snapshot: () => Promise<string>;
  click: (selector?: string, options?: { timeout?: number }) => Promise<string>;
  fill: (
    selector: string,
    value: string,
    options?: { timeout?: number },
  ) => Promise<string>;
  getText: (
    selector?: string,
    options?: { timeout?: number },
  ) => Promise<string>;
  count: (selector: string) => Promise<number>;
  keypress: (
    selector?: string,
    key?: string,
    options?: { timeout?: number },
  ) => Promise<string>;
  waitForText: (
    text: string,
    options?: { selector?: string; timeout?: number },
  ) => Promise<string>;
  scroll: (direction?: string) => Promise<string>;
  screenshot: (path?: string) => Promise<string>;
  close: () => void;
}

function send(socket: net.Socket, command: TestDriverCommand): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = "";

    const cleanup = () => {
      socket.removeListener("data", onData);
      socket.removeListener("error", onError);
      socket.removeListener("close", onClose);
    };

    const onData = (chunk: Buffer) => {
      data += chunk.toString();
      if (data.includes("\n")) {
        cleanup();
        try {
          const parsed: TestDriverResult = JSON.parse(data.trim());
          if (parsed.success) {
            resolve(parsed.data ?? "");
          } else {
            reject(new Error(parsed.error || "Unknown test driver error"));
          }
        } catch (_e) {
          reject(new Error(`Invalid response: ${data}`));
        }
      }
    };

    const onError = (err: Error) => {
      cleanup();
      reject(new Error(`Test driver socket error: ${err.message}`));
    };

    const onClose = () => {
      cleanup();
      reject(
        new Error("Test driver socket closed before response was received"),
      );
    };

    socket.on("data", onData);
    socket.on("error", onError);
    socket.on("close", onClose);
    socket.write(`${JSON.stringify(command)}\n`);
  });
}

/**
 * Create a client connection to the Tauri app test driver.
 * Returns an object with methods for each test driver command.
 */
export async function createTestDriver({
  port = PORT,
}: {
  port?: number;
} = {}): Promise<TestDriver> {
  const socket = net.createConnection({ port, host: "127.0.0.1" });

  await new Promise<void>((resolve, reject) => {
    socket.on("connect", resolve);
    socket.on("error", (err) => {
      reject(
        new Error(
          `Cannot connect to test driver on port ${port}. ` +
            `Is the Tauri app running with --features app-test-driver? (${err.message})`,
        ),
      );
    });
  });

  return {
    snapshot() {
      return send(socket, { action: "snapshot" });
    },
    click(selector?: string, options?: { timeout?: number }) {
      return send(socket, {
        action: "click",
        selector,
        timeout: options?.timeout,
      });
    },
    fill(selector: string, value: string, options?: { timeout?: number }) {
      return send(socket, {
        action: "fill",
        selector,
        value,
        timeout: options?.timeout,
      });
    },
    getText(selector?: string, options?: { timeout?: number }) {
      return send(socket, {
        action: "getText",
        selector,
        timeout: options?.timeout,
      });
    },
    count(selector: string) {
      return send(socket, { action: "count", selector }).then(Number);
    },
    keypress(selector?: string, key?: string, options?: { timeout?: number }) {
      return send(socket, {
        action: "keypress",
        selector,
        value: key,
        timeout: options?.timeout,
      });
    },
    waitForText(
      text: string,
      options?: { selector?: string; timeout?: number },
    ) {
      return send(socket, {
        action: "waitForText",
        selector: options?.selector ?? "body",
        value: text,
        timeout: options?.timeout ?? 30000,
      });
    },
    scroll(direction?: string) {
      return send(socket, { action: "scroll", value: direction });
    },
    screenshot(path?: string) {
      return send(socket, { action: "screenshot", value: path });
    },
    close() {
      socket.end();
    },
  };
}
