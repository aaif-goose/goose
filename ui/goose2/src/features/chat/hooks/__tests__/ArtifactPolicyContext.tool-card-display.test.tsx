import { render, screen } from "@testing-library/react";
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
      <span data-testid="write-secondary-paths">
        {writeDisplay.secondaryCandidates
          .map((candidate) => candidate.resolvedPath)
          .join(",")}
      </span>
      <span data-testid="cloned-role">{clonedDisplay.role}</span>
    </div>
  );
}

function EditWriteProbe({
  editArgs,
  writeArgs,
}: {
  editArgs: Record<string, unknown>;
  writeArgs: Record<string, unknown>;
}) {
  const { resolveToolCardDisplay } = useArtifactPolicyContext();
  const editDisplay = resolveToolCardDisplay(editArgs, "edit_file");
  const writeDisplay = resolveToolCardDisplay(writeArgs, "write_file");

  return (
    <div>
      <span data-testid="edit-role">{editDisplay.role}</span>
      <span data-testid="write-role">{writeDisplay.role}</span>
      <span data-testid="write-secondary-paths">
        {writeDisplay.secondaryCandidates
          .map((candidate) => candidate.resolvedPath)
          .join(",")}
      </span>
    </div>
  );
}

describe("ArtifactPolicyContext tool card display", () => {
  it("resolves tool card displays per tool call and by args identity", () => {
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
    expect(screen.getByTestId("write-secondary-paths")).toHaveTextContent(
      "/Users/test/project-a/output/notes.md",
    );
    expect(screen.getByTestId("write-secondary-paths")).not.toHaveTextContent(
      "/Users/test/project-a/notes.md",
    );
    expect(screen.getByTestId("cloned-role")).toHaveTextContent("none");
  });

  it("does not surface artifact actions for edit tool calls in mixed messages", () => {
    mockPathExists.mockReset();
    vi.mocked(openPath).mockReset();
    const editArgs = { path: "/Users/test/project-a/README.md" };
    const writeArgs = {
      paths: [
        "/Users/test/project-a/output/final_report.md",
        "/Users/test/project-a/output/notes.md",
      ],
    };
    const messages: Message[] = [
      {
        id: "assistant-edit-write",
        role: "assistant",
        created: Date.now(),
        content: [
          {
            type: "toolRequest",
            id: "tool-edit",
            name: "edit_file",
            arguments: editArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-edit",
            name: "edit_file",
            result: "Edited /Users/test/project-a/README.md",
            isError: false,
          },
          {
            type: "toolRequest",
            id: "tool-write",
            name: "write_file",
            arguments: writeArgs,
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-write",
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
        <EditWriteProbe editArgs={editArgs} writeArgs={writeArgs} />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("edit-role")).toHaveTextContent("none");
    expect(screen.getByTestId("write-role")).toHaveTextContent("primary_host");
    expect(screen.getByTestId("write-secondary-paths")).toHaveTextContent(
      "/Users/test/project-a/output/notes.md",
    );
    expect(screen.getByTestId("write-secondary-paths")).not.toHaveTextContent(
      "/Users/test/project-a/README.md",
    );
  });
});
