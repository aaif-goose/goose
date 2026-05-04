import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PersonaCard } from "../PersonaCard";
import type { Persona } from "@/shared/types/agents";

function makePersona(overrides: Partial<Persona> = {}): Persona {
  return {
    id: "p1",
    displayName: "Goose Default",
    systemPrompt: "You are a helpful assistant that writes code.",
    isBuiltin: false,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

describe("PersonaCard", () => {
  it("renders persona name", () => {
    render(<PersonaCard persona={makePersona({ displayName: "Coder" })} />);
    expect(screen.getByText("Coder")).toBeInTheDocument();
  });

  it("does not show a provenance badge", () => {
    render(<PersonaCard persona={makePersona({ isBuiltin: false })} />);
    expect(screen.queryByText("Featured")).not.toBeInTheDocument();
  });

  it("shows avatar with one initial for single-word names", () => {
    render(<PersonaCard persona={makePersona({ displayName: "Alpha" })} />);
    expect(screen.getByText("A")).toBeInTheDocument();
  });

  it("shows avatar with two initials for multi-word names", () => {
    render(
      <PersonaCard persona={makePersona({ displayName: "Code Reviewer" })} />,
    );
    expect(screen.getByText("CR")).toBeInTheDocument();
  });

  it("skips punctuation when building initials", () => {
    render(
      <PersonaCard
        persona={makePersona({ displayName: "404Portfolio (Copy)" })}
      />,
    );
    expect(screen.getByText("4C")).toBeInTheDocument();
  });

  it("shows system prompt preview", () => {
    render(
      <PersonaCard
        persona={makePersona({ systemPrompt: "You are a coding assistant." })}
      />,
    );
    expect(screen.getByText("You are a coding assistant.")).toBeInTheDocument();
  });

  it("calls onSelect on click", async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    const persona = makePersona();
    render(<PersonaCard persona={persona} onSelect={onSelect} />);

    await user.click(screen.getByLabelText(/^agent: /i));
    expect(onSelect).toHaveBeenCalledWith(persona);
  });

  it("shows dropdown menu on options button click", async () => {
    const user = userEvent.setup();
    render(
      <PersonaCard
        persona={makePersona({ sourcePath: "/tmp/code-review.md" })}
        onStartChat={vi.fn()}
        onEdit={vi.fn()}
        onDuplicate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /agent options/i }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /start a chat/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /edit/i })).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /share/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /duplicate/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /delete/i }),
    ).toBeInTheDocument();
  });

  it("shows delete for imported seeded personas", async () => {
    const user = userEvent.setup();
    render(
      <PersonaCard
        persona={makePersona({ isBuiltin: true })}
        onDelete={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /agent options/i }));
    expect(
      screen.getByRole("menuitem", { name: /delete/i }),
    ).toBeInTheDocument();
  });

  it("does not trigger selection when keyboard opens the options menu", async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(
      <PersonaCard
        persona={makePersona()}
        onSelect={onSelect}
        onDuplicate={vi.fn()}
      />,
    );

    screen.getByRole("button", { name: /agent options/i }).focus();
    await user.keyboard("{Enter}");

    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(onSelect).not.toHaveBeenCalled();
  });
});
