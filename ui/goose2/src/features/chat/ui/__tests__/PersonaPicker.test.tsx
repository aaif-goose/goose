import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { PersonaPicker } from "../PersonaPicker";
import type { Persona } from "@/shared/types/agents";

const PERSONAS: Persona[] = [
  {
    id: "solo",
    displayName: "Solo",
    systemPrompt:
      "You are an orchestration agent that decomposes complex tasks into smaller pieces so collaborators can move quickly and confidently.",
    isBuiltin: true,
    createdAt: "",
    updatedAt: "",
  },
];

describe("PersonaPicker", () => {
  it("shows the full first-sentence summary instead of pre-truncating it", async () => {
    const user = userEvent.setup();

    render(
      <PersonaPicker
        personas={PERSONAS}
        selectedPersonaId={null}
        onPersonaChange={vi.fn()}
        triggerVariant="icon"
      />,
    );

    await user.click(screen.getByRole("button", { name: /choose assistant/i }));

    expect(
      screen.getByText(
        "You are an orchestration agent that decomposes complex tasks into smaller pieces so collaborators can move quickly and confidently.",
      ),
    ).toBeInTheDocument();
  });
});
