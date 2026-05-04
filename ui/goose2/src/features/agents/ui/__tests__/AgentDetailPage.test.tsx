import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Persona } from "@/shared/types/agents";
import { AgentDetailPage } from "../AgentDetailPage";

function makePersona(overrides: Partial<Persona> = {}): Persona {
  return {
    id: "p1",
    displayName: "Code Reviewer",
    systemPrompt: "Review code for bugs.",
    isBuiltin: false,
    sourcePath: "/Users/test/.goose/agents/code-review.md",
    createdAt: "2026-04-01T00:00:00.000Z",
    updatedAt: "2026-04-02T00:00:00.000Z",
    ...overrides,
  };
}

function renderDetail(
  persona = makePersona(),
  overrides: Partial<ComponentProps<typeof AgentDetailPage>> = {},
) {
  const props: ComponentProps<typeof AgentDetailPage> = {
    persona,
    onBack: vi.fn(),
    onEdit: vi.fn(),
    onStartChat: vi.fn(),
    onCopyFile: vi.fn(),
    onSaveCopy: vi.fn(),
    onDuplicate: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  };

  render(<AgentDetailPage {...props} />);
  return props;
}

describe("AgentDetailPage", () => {
  it("shows labeled primary actions for file-backed agents", async () => {
    const user = userEvent.setup();
    const persona = makePersona();
    const props = renderDetail(persona);

    await user.click(screen.getByRole("button", { name: "Start a chat" }));
    await user.click(screen.getByRole("button", { name: "Edit" }));

    expect(props.onStartChat).toHaveBeenCalledWith(persona);
    expect(props.onEdit).toHaveBeenCalledWith(persona);
  });

  it("keeps file actions in the share menu", async () => {
    const user = userEvent.setup();
    renderDetail();

    await user.click(screen.getByRole("button", { name: "Share" }));

    expect(screen.getByRole("button", { name: "More" })).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "Copy file to clipboard" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "Export a copy" }),
    ).toBeInTheDocument();
  });
});
