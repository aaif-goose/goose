import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import {
  ArtifactPolicyProvider,
  useArtifactPolicyContext,
} from "../ArtifactPolicyContext";

import { openPath } from "@tauri-apps/plugin-opener";

const mockPathExists = vi.fn<(path: string) => Promise<boolean>>();

vi.mock("@/shared/api/system", () => ({
  pathExists: (path: string) => mockPathExists(path),
}));

function Probe({
  readArgs,
  writeArgs,
  clonedWriteArgs,
}: {
  readArgs: Record<string, unknown>;
  writeArgs: Record<string, unknown>;
  clonedWriteArgs: Record<string, unknown>;
}) {
  const { resolveToolCardDisplay } = useArtifactPolicyContext();
  const readDisplay = resolveToolCardDisplay(readArgs, "read_file");
  const writeDisplay = resolveToolCardDisplay(writeArgs, "write_file");
  const clonedDisplay = resolveToolCardDisplay(clonedWriteArgs, "write_file");

  return (
    <div>
      <span data-testid="read-role">{readDisplay.role}</span>
      <span data-testid="write-role">{writeDisplay.role}</span>
      <span data-testid="write-primary">
        {writeDisplay.primaryCandidate?.resolvedPath ?? ""}
      </span>
      <span data-testid="write-secondary-count">
        {String(writeDisplay.secondaryCandidates.length)}
      </span>
      <span data-testid="cloned-role">{clonedDisplay.role}</span>
    </div>
  );
}

function TextFollowupProbe({
  writeArgs,
}: {
  writeArgs: Record<string, unknown>;
}) {
  const { resolveToolCardDisplay, getAllSessionArtifacts } =
    useArtifactPolicyContext();
  const display = resolveToolCardDisplay(
    writeArgs,
    "writing markdown file about alphabet history",
  );
  const artifacts = getAllSessionArtifacts();

  return (
    <div>
      <span data-testid="text-followup-role">{display.role}</span>
      <span data-testid="text-followup-path">
        {display.primaryCandidate?.resolvedPath ?? ""}
      </span>
      <span data-testid="text-followup-artifacts">
        {artifacts.map((artifact) => artifact.resolvedPath).join(",")}
      </span>
    </div>
  );
}

function ReadOnlyProbe({ readArgs }: { readArgs: Record<string, unknown> }) {
  const { resolveToolCardDisplay, getAllSessionArtifacts } =
    useArtifactPolicyContext();
  const display = resolveToolCardDisplay(readArgs, "read_file");
  const artifacts = getAllSessionArtifacts();

  return (
    <div>
      <span data-testid="read-only-role">{display.role}</span>
      <span data-testid="read-only-artifacts">
        {artifacts.map((artifact) => artifact.resolvedPath).join(",")}
      </span>
    </div>
  );
}

function FallbackProbe({
  path = "/Users/test/.goose/projects/sample-project/artifacts/report.md",
}: {
  path?: string;
}) {
  const { pathExists, openResolvedPath } = useArtifactPolicyContext();

  return (
    <div>
      <button
        type="button"
        onClick={async () => {
          const exists = await pathExists(path);
          (window as Window & { __artifactExists?: boolean }).__artifactExists =
            exists;
        }}
      >
        Check path
      </button>
      <button type="button" onClick={() => void openResolvedPath(path)}>
        Open path
      </button>
    </div>
  );
}

describe("ArtifactPolicyContext", () => {
  it("computes one primary host per message and resolves tool cards by args identity", () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    const readArgs = { path: "/Users/test/project-a/notes.md" };
    const writeArgs = {
      paths: [
        "/Users/test/project-a/output/final_report.md",
        "/Users/test/project-a/output/notes.md",
      ],
    };
    const messages: Message[] = [
      {
        id: "assistant-1",
        role: "assistant",
        created: Date.now(),
        content: [
          {
            type: "toolRequest",
            id: "tool-1",
            name: "read_file",
            arguments: readArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-1",
            name: "read_file",
            result: "Read /Users/test/project-a/notes.md",
            isError: false,
          },
          {
            type: "toolRequest",
            id: "tool-2",
            name: "write_file",
            arguments: writeArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-2",
            name: "write_file",
            result: "Created /Users/test/project-a/output/final_report.md",
            isError: false,
          },
        ],
      },
    ];

    render(
      <ArtifactPolicyProvider
        messages={messages}
        allowedRoots={["/Users/test/project-a", "/Users/test/.goose/artifacts"]}
      >
        <Probe
          readArgs={readArgs}
          writeArgs={writeArgs}
          clonedWriteArgs={{ ...writeArgs }}
        />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("read-role")).toHaveTextContent("none");
    expect(screen.getByTestId("write-role")).toHaveTextContent("primary_host");
    expect(screen.getByTestId("write-primary")).toHaveTextContent(
      "/Users/test/project-a/output/final_report.md",
    );
    expect(
      Number(screen.getByTestId("write-secondary-count").textContent),
    ).toBeGreaterThan(0);
    expect(screen.getByTestId("cloned-role")).toHaveTextContent("none");
  });

  it("does not treat read-only tool paths as session artifacts", () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    const readArgs = { path: "/Users/test/project-a/notes.md" };
    const messages: Message[] = [
      {
        id: "assistant-read-only",
        role: "assistant",
        created: Date.now(),
        content: [
          {
            type: "toolRequest",
            id: "tool-read",
            name: "read_file",
            arguments: readArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-read",
            name: "read_file",
            result: "Read /Users/test/project-a/notes.md",
            isError: false,
          },
        ],
      },
    ];

    render(
      <ArtifactPolicyProvider
        messages={messages}
        allowedRoots={["/Users/test/project-a", "/Users/test/.goose/artifacts"]}
      >
        <ReadOnlyProbe readArgs={readArgs} />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("read-only-role")).toHaveTextContent("none");
    expect(screen.getByTestId("read-only-artifacts")).toHaveTextContent("");
  });

  it("falls back to the home artifacts root when a project artifacts path is missing", async () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    mockPathExists.mockImplementation(
      async (path: string) => path === "/Users/test/.goose/artifacts/report.md",
    );

    render(
      <ArtifactPolicyProvider
        messages={[]}
        allowedRoots={[
          "/Users/test/.goose/projects/sample-project/artifacts",
          "/Users/test/.goose/artifacts",
        ]}
      >
        <FallbackProbe />
      </ArtifactPolicyProvider>,
    );

    screen.getByRole("button", { name: "Check path" }).click();
    await waitFor(() => {
      expect(
        (window as Window & { __artifactExists?: boolean }).__artifactExists,
      ).toBe(true);
    });

    screen.getByRole("button", { name: "Open path" }).click();
    await waitFor(() => {
      expect(vi.mocked(openPath)).toHaveBeenCalledWith(
        "/Users/test/.goose/artifacts/report.md",
      );
    });
  });

  it("falls back from a working-dir artifacts path to the project root when the file lives there", async () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    mockPathExists.mockImplementation(
      async (path: string) =>
        path === "/Users/test/project-a/README_ENHANCED.md",
    );

    render(
      <ArtifactPolicyProvider
        messages={[]}
        allowedRoots={[
          "/Users/test/project-a/artifacts",
          "/Users/test/project-a",
          "/Users/test/.goose/artifacts",
        ]}
      >
        <FallbackProbe path="/Users/test/project-a/artifacts/README_ENHANCED.md" />
      </ArtifactPolicyProvider>,
    );

    screen.getByRole("button", { name: "Check path" }).click();
    await waitFor(() => {
      expect(
        (window as Window & { __artifactExists?: boolean }).__artifactExists,
      ).toBe(true);
    });

    screen.getByRole("button", { name: "Open path" }).click();
    await waitFor(() => {
      expect(vi.mocked(openPath)).toHaveBeenCalledWith(
        "/Users/test/project-a/README_ENHANCED.md",
      );
    });
  });

  it("does not strip /artifacts/ from a parent directory in the path", async () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    // The file lives at the nested artifacts path — the parent `/artifacts/` should NOT be stripped
    mockPathExists.mockImplementation(
      async (path: string) =>
        path === "/Users/test/artifacts/project/artifacts/README_ENHANCED.md",
    );

    render(
      <ArtifactPolicyProvider
        messages={[]}
        allowedRoots={[
          "/Users/test/artifacts/project/artifacts",
          "/Users/test/artifacts/project",
          "/Users/test/.goose/artifacts",
        ]}
      >
        <FallbackProbe path="/Users/test/artifacts/project/artifacts/README_ENHANCED.md" />
      </ArtifactPolicyProvider>,
    );

    screen.getByRole("button", { name: "Check path" }).click();
    await waitFor(() => {
      expect(
        (window as Window & { __artifactExists?: boolean }).__artifactExists,
      ).toBe(true);
    });

    screen.getByRole("button", { name: "Open path" }).click();
    await waitFor(() => {
      expect(vi.mocked(openPath)).toHaveBeenCalledWith(
        "/Users/test/artifacts/project/artifacts/README_ENHANCED.md",
      );
    });
  });

  it("falls back correctly when /artifacts/ appears in a parent dir", async () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    // File is NOT at the artifacts path, but IS at the root-stripped path
    mockPathExists.mockImplementation(
      async (path: string) =>
        path === "/Users/test/artifacts/project/README.md",
    );

    render(
      <ArtifactPolicyProvider
        messages={[]}
        allowedRoots={[
          "/Users/test/artifacts/project/artifacts",
          "/Users/test/artifacts/project",
          "/Users/test/.goose/artifacts",
        ]}
      >
        <FallbackProbe path="/Users/test/artifacts/project/artifacts/README.md" />
      </ArtifactPolicyProvider>,
    );

    screen.getByRole("button", { name: "Check path" }).click();
    await waitFor(() => {
      expect(
        (window as Window & { __artifactExists?: boolean }).__artifactExists,
      ).toBe(true);
    });

    screen.getByRole("button", { name: "Open path" }).click();
    await waitFor(() => {
      expect(vi.mocked(openPath)).toHaveBeenCalledWith(
        "/Users/test/artifacts/project/README.md",
      );
    });
  });

  it("uses assistant text after a tool call to populate file actions and the Files tab", () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    const writeArgs = {};
    const messages: Message[] = [
      {
        id: "assistant-1",
        role: "assistant",
        created: Date.now(),
        metadata: { userVisible: true, agentVisible: true },
        content: [
          {
            type: "toolRequest",
            id: "tool-1",
            name: "writing markdown file about alphabet history",
            arguments: writeArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-1",
            name: "writing markdown file about alphabet history",
            result: "completed",
            isError: false,
          },
          {
            type: "text",
            text: "The file alpha.md has been created at /Users/test/.goose/artifacts/alpha.md.",
          },
        ],
      },
    ];

    render(
      <ArtifactPolicyProvider
        messages={messages}
        allowedRoots={["/Users/test/.goose/artifacts"]}
      >
        <TextFollowupProbe writeArgs={writeArgs} />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("text-followup-role")).toHaveTextContent(
      "primary_host",
    );
    expect(screen.getByTestId("text-followup-path")).toHaveTextContent(
      "/Users/test/.goose/artifacts/alpha.md",
    );
    expect(screen.getByTestId("text-followup-artifacts")).toHaveTextContent(
      "/Users/test/.goose/artifacts/alpha.md",
    );
  });
});
