import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ChatInput } from "../ChatInput";

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

vi.mock("@/shared/api/system", () => ({
  listFilesForMentions: vi.fn().mockResolvedValue([]),
}));

type SkillMentionFixture = {
  id: string;
  name: string;
  description: string;
  sourceLabel: string;
};
const mockListSkills = vi.fn<
  (projectDirs?: string[]) => Promise<SkillMentionFixture[]>
>(async () => []);
vi.mock("@/features/skills/api/skills", () => ({
  listSkills: (projectDirs?: string[]) => mockListSkills(projectDirs),
}));

describe("ChatInput skill mentions", () => {
  beforeEach(() => {
    mockListSkills.mockClear();
    mockListSkills.mockResolvedValue([]);
    mockVoiceDictation.isStarting.mockReset();
    mockVoiceDictation.isStarting.mockReturnValue(false);
  });

  it("shows skills in @mention results and creates a skill chip", async () => {
    const user = userEvent.setup();
    mockListSkills.mockResolvedValue([
      {
        id: "global:/skills/code-review",
        name: "code-review",
        description: "Reviews code",
        sourceLabel: "Personal",
      },
    ]);

    render(<ChatInput onSend={vi.fn()} />);

    await waitFor(() => {
      expect(mockListSkills).toHaveBeenCalled();
    });

    const input = screen.getByRole("textbox");
    await user.type(input, "@code");

    expect(await screen.findByText("Skills")).toBeInTheDocument();

    await user.click(
      await screen.findByRole("option", { name: /code-review/i }),
    );

    expect(input).toHaveValue("");
    expect(screen.getByText("code-review")).toBeInTheDocument();
  });

  it("expands selected skill chips before sending", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();

    render(
      <ChatInput
        onSend={onSend}
        selectedSkills={[
          {
            id: "global:/skills/code-review",
            name: "code-review",
            description: "Reviews code",
            sourceLabel: "Personal",
          },
        ]}
        onSkillsChange={vi.fn()}
      />,
    );

    await user.type(screen.getByRole("textbox"), "check this diff");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith(
      "check this diff",
      undefined,
      undefined,
      {
        assistantPrompt: "Use these skills for this request: code-review.",
        chips: [{ label: "code-review", type: "skill" }],
        displayText: "check this diff",
      },
    );
  });

  it("expands direct slash skill commands before sending", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    mockListSkills.mockResolvedValue([
      {
        id: "global:/skills/code-review",
        name: "code-review",
        description: "Reviews code",
        sourceLabel: "Personal",
      },
    ]);

    render(<ChatInput onSend={onSend} />);

    await waitFor(() => {
      expect(mockListSkills).toHaveBeenCalled();
    });

    const input = screen.getByRole("textbox");
    await user.type(input, "/code-review check this diff");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith(
      "check this diff",
      undefined,
      undefined,
      {
        assistantPrompt: "Use these skills for this request: code-review.",
        chips: [{ label: "code-review", type: "skill" }],
        displayText: "check this diff",
      },
    );
  });

  it("does not expand reserved slash commands as skills", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    mockListSkills.mockResolvedValue([
      {
        id: "global:/skills/compact",
        name: "compact",
        description: "A compacting skill",
        sourceLabel: "Personal",
      },
    ]);

    render(<ChatInput onSend={onSend} />);

    await waitFor(() => {
      expect(mockListSkills).toHaveBeenCalled();
    });

    const input = screen.getByRole("textbox");
    await user.type(input, "/compact");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("/compact", undefined, undefined);
  });
});
