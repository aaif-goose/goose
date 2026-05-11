import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { acpFsCallbacks } from "../acpFsCallbacks";

const mockInvoke = vi.mocked(invoke);

describe("acpFsCallbacks", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  describe("readTextFile", () => {
    it("delegates to acp_read_text_file Tauri command", async () => {
      mockInvoke.mockResolvedValueOnce("hello world\n");

      const result = await acpFsCallbacks.readTextFile({
        sessionId: "session-1",
        path: "/tmp/example.txt",
      });

      expect(mockInvoke).toHaveBeenCalledWith("acp_read_text_file", {
        path: "/tmp/example.txt",
        line: null,
        limit: null,
      });
      expect(result).toEqual({ content: "hello world\n" });
    });

    it("forwards line and limit when provided", async () => {
      mockInvoke.mockResolvedValueOnce("b\nc\n");

      const result = await acpFsCallbacks.readTextFile({
        sessionId: "session-1",
        path: "/tmp/example.txt",
        line: 2,
        limit: 2,
      });

      expect(mockInvoke).toHaveBeenCalledWith("acp_read_text_file", {
        path: "/tmp/example.txt",
        line: 2,
        limit: 2,
      });
      expect(result).toEqual({ content: "b\nc\n" });
    });

    it("propagates errors from the Tauri command", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("No such file"));

      await expect(
        acpFsCallbacks.readTextFile({
          sessionId: "session-1",
          path: "/tmp/missing.txt",
        }),
      ).rejects.toThrow("No such file");
    });
  });

  describe("writeTextFile", () => {
    it("delegates to acp_write_text_file Tauri command", async () => {
      mockInvoke.mockResolvedValueOnce(undefined);

      const result = await acpFsCallbacks.writeTextFile({
        sessionId: "session-1",
        path: "/tmp/out.txt",
        content: "payload",
      });

      expect(mockInvoke).toHaveBeenCalledWith("acp_write_text_file", {
        path: "/tmp/out.txt",
        content: "payload",
      });
      expect(result).toEqual({});
    });

    it("propagates errors from the Tauri command", async () => {
      mockInvoke.mockRejectedValueOnce(new Error("Permission denied"));

      await expect(
        acpFsCallbacks.writeTextFile({
          sessionId: "session-1",
          path: "/forbidden",
          content: "",
        }),
      ).rejects.toThrow("Permission denied");
    });
  });
});
