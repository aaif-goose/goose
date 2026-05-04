import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import {
  ArtifactPolicyProvider,
  useArtifactPolicyContext,
} from "../ArtifactPolicyContext";

const mockPathExists = vi.fn<(path: string) => Promise<boolean>>();

vi.mock("@/shared/api/system", () => ({
  pathExists: (path: string) => mockPathExists(path),
}));

function ArtifactsProbe() {
  const { getAllSessionArtifacts } = useArtifactPolicyContext();
  const artifacts = getAllSessionArtifacts();

  return (
    <div>
      <span data-testid="artifact-paths">
        {artifacts.map((artifact) => artifact.resolvedPath).join(",")}
      </span>
      <span data-testid="artifact-count">{String(artifacts.length)}</span>
    </div>
  );
}

function LinkProbe({ href }: { href: string }) {
  const { resolveMarkdownHref } = useArtifactPolicyContext();
  const candidate = resolveMarkdownHref(href);

  return (
    <div>
      <span data-testid="link-path">{candidate?.resolvedPath ?? ""}</span>
    </div>
  );
}

describe("ArtifactPolicyContext", () => {
  it("uses reported ACP tool locations as session artifacts", () => {
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
            arguments: {},
            status: "completed",
            toolKind: "read",
            locations: [{ path: "/Users/test/project-a/notes.md" }],
          },
          {
            type: "toolResponse",
            id: "tool-1",
            name: "read_file",
            result: "Read notes",
            isError: false,
          },
        ],
      },
    ];

    render(
      <ArtifactPolicyProvider
        messages={messages}
        sessionCwd="/Users/test/project-a"
      >
        <ArtifactsProbe />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("artifact-count")).toHaveTextContent("1");
    expect(screen.getByTestId("artifact-paths")).toHaveTextContent(
      "/Users/test/project-a/notes.md",
    );
  });

  it("does not filter reported locations outside allowed roots", () => {
    const messages: Message[] = [
      {
        id: "assistant-1",
        role: "assistant",
        created: Date.now(),
        content: [
          {
            type: "toolRequest",
            id: "tool-1",
            name: "write_file",
            arguments: {},
            status: "completed",
            toolKind: "edit",
            locations: [{ path: "/tmp/outside.md" }],
          },
        ],
      },
    ];

    render(
      <ArtifactPolicyProvider
        messages={messages}
        sessionCwd="/Users/test/project-a"
      >
        <ArtifactsProbe />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("artifact-paths")).toHaveTextContent(
      "/tmp/outside.md",
    );
  });

  it("resolves local markdown hrefs relative to the session cwd", () => {
    render(
      <ArtifactPolicyProvider messages={[]} sessionCwd="/Users/test/app">
        <LinkProbe href="output/report.md" />
      </ArtifactPolicyProvider>,
    );

    expect(screen.getByTestId("link-path")).toHaveTextContent(
      "/Users/test/app/output/report.md",
    );
  });
});
