import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ChatInput } from "./chatInputTestUtils";

const mockVoiceDictation = {
  isEnabled: true,
  isRecording: false,
  isTranscribing: false,
  isStarting: vi.fn(() => false),
  stopRecording: vi.fn(),
  toggleRecording: vi.fn(),
};

vi.mock("../hooks/useVoiceDictation", () => ({
  useVoiceDictation: () => mockVoiceDictation,
}));

vi.mock("@/features/providers/hooks/useAgentProviderStatus", () => ({
  useAgentProviderStatus: () => ({
    readyAgentIds: new Set(["goose", "claude-acp", "codex-acp"]),
    loading: false,
    refresh: vi.fn(),
  }),
}));

const mockListFilesForMentions = vi.fn<
  (roots: string[], maxResults?: number) => Promise<string[]>
>(async () => []);

vi.mock("@/shared/api/system", () => ({
  listFilesForMentions: (roots: string[], maxResults?: number) =>
    mockListFilesForMentions(roots, maxResults),
}));

vi.mock("@/features/skills/api/skills", () => ({
  listSkills: vi.fn().mockResolvedValue([]),
}));

describe("ChatInput async send handling", () => {
  beforeEach(() => {
    mockListFilesForMentions.mockClear();
    mockListFilesForMentions.mockResolvedValue([]);
    mockVoiceDictation.isEnabled = true;
    mockVoiceDictation.isRecording = false;
    mockVoiceDictation.isTranscribing = false;
    mockVoiceDictation.isStarting.mockReset();
    mockVoiceDictation.isStarting.mockReturnValue(false);
    mockVoiceDictation.stopRecording.mockReset();
    mockVoiceDictation.toggleRecording.mockReset();
  });

  it("clears the composer after an accepted async send when the draft is unchanged", async () => {
    let resolveSend!: (accepted: boolean) => void;
    const onSend = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveSend = resolve;
        }),
    );
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    await user.keyboard("{Enter}");

    resolveSend(true);

    await waitFor(() => {
      expect(input).toHaveValue("");
    });
  });

  it("preserves newer draft text when an async send resolves later", async () => {
    let resolveSend!: (accepted: boolean) => void;
    const onSend = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveSend = resolve;
        }),
    );
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    await user.keyboard("{Enter}");
    await user.type(input, " world");

    resolveSend(true);

    await waitFor(() => {
      expect(input).toHaveValue("hello world");
    });
  });
});
