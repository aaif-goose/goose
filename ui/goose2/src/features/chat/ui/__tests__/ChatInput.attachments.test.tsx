import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ChatInput } from "../ChatInput";

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
const mockInspectAttachmentPaths = vi.fn<
  (paths: string[]) => Promise<
    {
      name: string;
      path: string;
      kind: "file" | "directory";
      mimeType?: string | null;
    }[]
  >
>(async () => []);
const mockReadImageAttachment = vi.fn<
  (path: string) => Promise<{ base64: string; mimeType: string }>
>(async () => ({ base64: "abc", mimeType: "image/png" }));

vi.mock("@/shared/api/system", () => ({
  listFilesForMentions: (roots: string[], maxResults?: number) =>
    mockListFilesForMentions(roots, maxResults),
  inspectAttachmentPaths: (paths: string[]) =>
    mockInspectAttachmentPaths(paths),
  readImageAttachment: (path: string) => mockReadImageAttachment(path),
}));

const mockOpenDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => mockOpenDialog(...args),
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

describe("ChatInput attachments", () => {
  beforeEach(() => {
    mockListFilesForMentions.mockClear();
    mockListFilesForMentions.mockResolvedValue([]);
    mockInspectAttachmentPaths.mockClear();
    mockInspectAttachmentPaths.mockResolvedValue([]);
    mockReadImageAttachment.mockClear();
    mockReadImageAttachment.mockResolvedValue({
      base64: "abc",
      mimeType: "image/png",
    });
    mockOpenDialog.mockClear();
    mockOpenDialog.mockResolvedValue(null);
  });

  it("attaches a file from the toolbar menu and sends it without text", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    mockOpenDialog.mockResolvedValue("/Users/test/report.pdf");
    mockInspectAttachmentPaths.mockResolvedValue([
      {
        name: "report.pdf",
        path: "/Users/test/report.pdf",
        kind: "file",
        mimeType: "application/pdf",
      },
    ]);

    render(<ChatInput onSend={onSend} />);

    await user.click(screen.getByRole("button", { name: /attach/i }));
    await user.click(screen.getByRole("menuitem", { name: /^file$/i }));

    expect(mockOpenDialog).toHaveBeenCalledWith({
      title: "Choose files to attach",
      multiple: true,
    });
    expect(await screen.findByText("report.pdf")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /send message/i }));

    expect(onSend).toHaveBeenCalledWith(
      "",
      undefined,
      expect.arrayContaining([
        expect.objectContaining({
          kind: "file",
          name: "report.pdf",
          path: "/Users/test/report.pdf",
        }),
      ]),
    );
  });

  it("attaches a folder from the toolbar menu", async () => {
    const user = userEvent.setup();
    mockOpenDialog.mockResolvedValue("/Users/test/screenshots");
    mockInspectAttachmentPaths.mockResolvedValue([
      {
        name: "screenshots",
        path: "/Users/test/screenshots",
        kind: "directory",
      },
    ]);

    render(<ChatInput onSend={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: /attach/i }));
    await user.click(screen.getByRole("menuitem", { name: /folder/i }));

    expect(mockOpenDialog).toHaveBeenCalledWith({
      directory: true,
      title: "Choose folders to attach",
      multiple: true,
    });
    expect(await screen.findByText("screenshots")).toBeInTheDocument();
  });

  it("shows the generic attachment drop overlay for file drags", () => {
    render(<ChatInput onSend={vi.fn()} />);

    const textbox = screen.getByRole("textbox");
    const composer = textbox.closest("div.rounded-2xl");
    if (!composer) {
      throw new Error("Expected composer container");
    }
    const dataTransfer = {
      files: [new File(["hello"], "report.txt", { type: "text/plain" })],
      items: [{ kind: "file" }],
      types: ["Files"],
    } as unknown as DataTransfer;

    fireEvent.dragEnter(composer, { dataTransfer });
    fireEvent.dragOver(composer, { dataTransfer });

    expect(
      screen.getByText("Drop files or folders to attach"),
    ).toBeInTheDocument();
  });

  it("renders mixed attachments from a single file picker pass", async () => {
    const user = userEvent.setup();
    mockOpenDialog.mockResolvedValue([
      "/Users/test/report.pdf",
      "/Users/test/diagram.png",
    ]);
    mockInspectAttachmentPaths.mockResolvedValue([
      {
        name: "report.pdf",
        path: "/Users/test/report.pdf",
        kind: "file",
        mimeType: "application/pdf",
      },
      {
        name: "diagram.png",
        path: "/Users/test/diagram.png",
        kind: "file",
        mimeType: "image/png",
      },
    ]);

    render(<ChatInput onSend={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: /attach/i }));
    await user.click(screen.getByRole("menuitem", { name: /^file$/i }));

    expect(await screen.findByText("report.pdf")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByAltText("Attachment 2")).toBeInTheDocument();
    });
  });
});
