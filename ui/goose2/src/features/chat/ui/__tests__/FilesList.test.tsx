import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FilesList } from "../FilesList";

import { openPath } from "@tauri-apps/plugin-opener";

const { mockListDirectoryEntries, mockRevealInFileManager } = vi.hoisted(
  () => ({
    mockListDirectoryEntries: vi.fn(),
    mockRevealInFileManager: vi.fn(),
  }),
);

vi.mock("@/shared/api/system", () => ({
  listDirectoryEntries: mockListDirectoryEntries,
}));

vi.mock("@/shared/lib/fileManager", () => ({
  revealInFileManager: mockRevealInFileManager,
}));

const makeEntry = (overrides: Record<string, unknown> = {}) => ({
  kind: "file" as const,
  name: "README.md",
  path: "/Users/test/project/README.md",
  ...overrides,
});

describe("FilesList", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(openPath).mockResolvedValue(undefined);
    mockRevealInFileManager.mockResolvedValue(undefined);
    mockListDirectoryEntries.mockResolvedValue([]);
  });

  it("shows an empty state when no project working directories are available", () => {
    render(<FilesList />);

    expect(
      screen.getByText(
        "Project files are unavailable until a project with working directories is assigned.",
      ),
    ).toBeInTheDocument();
  });

  it("renders separate top-level roots for each working directory", async () => {
    render(
      <FilesList
        projectWorkingDirs={["/Users/test/goose2", "/Users/test/sprout"]}
      />,
    );

    await waitFor(() => {
      expect(mockListDirectoryEntries).toHaveBeenCalledWith(
        "/Users/test/goose2",
      );
      expect(mockListDirectoryEntries).toHaveBeenCalledWith(
        "/Users/test/sprout",
      );
    });

    expect(screen.getByText("goose2")).toBeInTheDocument();
    expect(screen.getByText("sprout")).toBeInTheDocument();
  });

  it("expands folders in place without opening them externally", async () => {
    const user = userEvent.setup();
    mockListDirectoryEntries.mockImplementation((path: string) => {
      if (path === "/Users/test/project") {
        return Promise.resolve([
          makeEntry({
            kind: "directory",
            name: "src",
            path: "/Users/test/project/src",
          }),
        ]);
      }

      if (path === "/Users/test/project/src") {
        return Promise.resolve([
          makeEntry({
            name: "App.tsx",
            path: "/Users/test/project/src/App.tsx",
          }),
        ]);
      }

      return Promise.resolve([]);
    });

    render(<FilesList projectWorkingDirs={["/Users/test/project"]} />);

    await screen.findByText("src");
    await user.click(screen.getByText("src"));

    await waitFor(() => {
      expect(mockListDirectoryEntries).toHaveBeenCalledWith(
        "/Users/test/project/src",
      );
    });
    expect(vi.mocked(openPath)).not.toHaveBeenCalled();
    expect(screen.getByText("App.tsx")).toBeInTheDocument();
  });

  it("opens files externally when a file is clicked", async () => {
    const user = userEvent.setup();
    mockListDirectoryEntries.mockResolvedValue([
      makeEntry({
        name: "README.md",
        path: "/Users/test/project/README.md",
      }),
    ]);

    render(<FilesList projectWorkingDirs={["/Users/test/project"]} />);

    await user.click(await screen.findByText("README.md"));

    expect(vi.mocked(openPath)).toHaveBeenCalledWith(
      "/Users/test/project/README.md",
    );
  });

  it("supports context menu actions for folders and files", async () => {
    const user = userEvent.setup();
    mockListDirectoryEntries.mockResolvedValue([
      makeEntry({
        kind: "directory",
        name: "src",
        path: "/Users/test/project/src",
      }),
      makeEntry({
        name: "README.md",
        path: "/Users/test/project/README.md",
      }),
    ]);

    render(<FilesList projectWorkingDirs={["/Users/test/project"]} />);

    const folderLabel = await screen.findByText("src");
    fireEvent.contextMenu(folderLabel);
    await user.click(
      screen.getByRole("menuitem", {
        name: /reveal in (finder|explorer|file manager)/i,
      }),
    );
    expect(mockRevealInFileManager).toHaveBeenCalledWith(
      "/Users/test/project/src",
    );

    const fileLabel = screen.getByText("README.md");
    fireEvent.contextMenu(fileLabel);
    await user.click(
      screen.getByRole("menuitem", {
        name: /reveal in (finder|explorer|file manager)/i,
      }),
    );
    expect(mockRevealInFileManager).toHaveBeenCalledWith(
      "/Users/test/project/README.md",
    );
  });
});
