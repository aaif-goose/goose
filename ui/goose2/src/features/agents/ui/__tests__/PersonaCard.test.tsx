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

  it("shows built-in badge", () => {
    render(<PersonaCard persona={makePersona({ isBuiltin: true })} />);
    expect(screen.getByText("Built-in")).toBeInTheDocument();
  });

  it("does not show built-in badge for custom personas", () => {
    render(<PersonaCard persona={makePersona({ isBuiltin: false })} />);
    expect(screen.queryByText("Built-in")).not.toBeInTheDocument();
  });

  it("shows avatar with initial", () => {
    render(<PersonaCard persona={makePersona({ displayName: "Alpha" })} />);
    expect(screen.getByText("A")).toBeInTheDocument();
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

    await user.click(screen.getByLabelText(/^persona: /i));
    expect(onSelect).toHaveBeenCalledWith(persona);
  });

  it("shows dropdown menu on options button click", async () => {
    const user = userEvent.setup();
    render(
      <PersonaCard
        persona={makePersona()}
        onEdit={vi.fn()}
        onDuplicate={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /persona options/i }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /edit/i })).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /duplicate/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /delete/i }),
    ).toBeInTheDocument();
  });

  it("delete is disabled for built-in personas", async () => {
    const user = userEvent.setup();
    render(
      <PersonaCard
        persona={makePersona({ isBuiltin: true })}
        onDelete={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /persona options/i }));
    const deleteBtn = screen.queryByRole("menuitem", { name: /delete/i });
    expect(deleteBtn).toBeNull();
  });
});
