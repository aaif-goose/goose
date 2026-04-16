import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AgentModelPicker } from "../AgentModelPicker";

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver ??=
  ResizeObserverStub as unknown as typeof ResizeObserver;

const AGENTS = [
  { id: "goose", label: "Goose" },
  { id: "codex-acp", label: "Codex" },
];

describe("AgentModelPicker", () => {
  it("shows the selected agent and model in the trigger", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="gpt-4o"
        currentModelName="GPT-4o"
        availableModels={[{ id: "gpt-4o", name: "GPT-4o" }]}
        onModelChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("GPT-4o");
  });

  it("calls onModelChange when a model is selected", async () => {
    const user = userEvent.setup();
    const onModelChange = vi.fn();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="claude-sonnet-4"
        currentModelName="Claude Sonnet 4"
        availableModels={[
          { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
          { id: "gpt-4o", name: "GPT-4o" },
        ]}
        onModelChange={onModelChange}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    await user.click(screen.getByRole("button", { name: /OpenAI/i }));
    await user.click(screen.getByRole("button", { name: "GPT-4o" }));

    expect(onModelChange).toHaveBeenCalledWith("gpt-4o");
  });

  it("auto-expands the group containing the selected model", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="claude-sonnet-4"
        currentModelName="Claude Sonnet 4"
        availableModels={[
          { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
          { id: "gpt-4o", name: "GPT-4o" },
        ]}
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    expect(
      screen.getByRole("button", { name: "Claude Sonnet 4" }),
    ).toBeInTheDocument();
  });

  it("keeps long model names in constrained rows", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="databricks-gpt-5-4-mini"
        currentModelName="databricks-gpt-5-4-mini"
        availableModels={[
          {
            id: "databricks-gpt-5-4-mini",
            name: "databricks-gpt-5-4-mini",
            provider: "OpenAI",
          },
          {
            id: "databricks-gpt-5-4-nano-preview-super-long",
            name: "databricks-gpt-5-4-nano-preview-super-long",
            provider: "OpenAI",
          },
        ]}
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    const longModelButton = screen.getByRole("button", {
      name: "databricks-gpt-5-4-mini",
    });

    expect(longModelButton).toHaveClass("min-w-0");
    expect(longModelButton).toHaveClass("overflow-hidden");
  });

  it("shows only agent name when no model info is available", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId={null}
        currentModelName={null}
        availableModels={[]}
        onModelChange={vi.fn()}
      />,
    );

    const trigger = screen.getByRole("button", {
      name: /choose agent and model/i,
    });
    expect(trigger).toHaveTextContent("Goose");
    expect(trigger).not.toHaveTextContent("·");
  });
});
