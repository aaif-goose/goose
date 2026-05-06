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
  { id: "claude-acp", label: "Claude Code" },
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

  it("uses the selected agent label while a raw model id is unresolved", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="claude-acp"
        onAgentChange={vi.fn()}
        currentModelId="opus"
        currentModelProviderId="claude-acp"
        currentModelName="opus"
        availableModels={[]}
        onModelChange={vi.fn()}
      />,
    );

    const trigger = screen.getByRole("button", {
      name: /choose agent and model/i,
    });
    expect(trigger).toHaveTextContent("Claude Code");
    expect(trigger).not.toHaveTextContent("opus");
  });

  it("uses the inventory model label for a matching raw model id", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="claude-acp"
        onAgentChange={vi.fn()}
        currentModelId="opus"
        currentModelProviderId="claude-acp"
        currentModelName="opus"
        availableModels={[{ id: "opus", name: "Claude Opus 4.6" }]}
        onModelChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("Claude Opus 4.6");
  });

  it("uses a stored human model name before inventory resolves", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="claude-acp"
        onAgentChange={vi.fn()}
        currentModelId="opus"
        currentModelProviderId="claude-acp"
        currentModelName="Claude Opus 4.6"
        availableModels={[]}
        onModelChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("Claude Opus 4.6");
  });

  it("allows id-as-display-name labels after inventory resolves", () => {
    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="gpt-5.4"
        currentModelName="gpt-5.4"
        availableModels={[{ id: "gpt-5.4", name: "gpt-5.4" }]}
        onModelChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("gpt-5.4");
  });

  it("does not show a raw model id in the loading row", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="claude-acp"
        onAgentChange={vi.fn()}
        currentModelId="opus"
        currentModelProviderId="claude-acp"
        currentModelName="opus"
        availableModels={[]}
        modelsLoading
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    expect(screen.getByText("Loading models...")).toBeInTheDocument();
    expect(screen.queryByText("opus")).not.toBeInTheDocument();
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

    await user.click(screen.getByRole("button", { name: "GPT-4o" }));

    expect(onModelChange).toHaveBeenCalledWith(
      "gpt-4o",
      expect.objectContaining({ id: "gpt-4o" }),
    );
  });

  it("passes the clicked model option through for duplicate model ids", async () => {
    const user = userEvent.setup();
    const onModelChange = vi.fn();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="llama3.2"
        currentModelProviderId="custom_ollama"
        currentModelName="llama3.2"
        availableModels={[
          {
            id: "llama3.2",
            name: "llama3.2",
            providerId: "ollama",
            providerName: "Ollama",
            recommended: true,
          },
          {
            id: "llama3.2",
            name: "llama3.2",
            providerId: "custom_ollama",
            providerName: "Custom Ollama",
            recommended: true,
          },
        ]}
        onModelChange={onModelChange}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    const duplicateModelRows = screen.getAllByRole("button", {
      name: "llama3.2",
    });

    const selectedDuplicateRows = duplicateModelRows.filter((row) =>
      row.classList.contains("bg-muted/60"),
    );
    expect(selectedDuplicateRows).toHaveLength(1);

    await user.click(selectedDuplicateRows[0]);

    expect(onModelChange).toHaveBeenCalledWith(
      "llama3.2",
      expect.objectContaining({
        name: "llama3.2",
        providerId: "custom_ollama",
      }),
    );
  });

  it("does not select providerless duplicate rows when the current provider is known", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="llama3.2"
        currentModelProviderId="custom_ollama"
        currentModelName="llama3.2"
        availableModels={[
          {
            id: "llama3.2",
            name: "llama3.2",
            recommended: true,
          },
          {
            id: "llama3.2",
            name: "llama3.2",
            providerId: "custom_ollama",
            providerName: "Custom Ollama",
            recommended: true,
          },
        ]}
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    const duplicateModelRows = screen.getAllByRole("button", {
      name: "llama3.2",
    });

    expect(
      duplicateModelRows.filter((row) => row.classList.contains("bg-muted/60")),
    ).toHaveLength(1);
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

  it("disables spellcheck in the all-models search field", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId="claude-sonnet-4"
        currentModelName="Claude Sonnet 4"
        availableModels={[
          { id: "claude-sonnet-4", name: "Claude Sonnet 4", recommended: true },
          { id: "gpt-4o-mini-2024-07-18", name: "GPT-4o mini" },
        ]}
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );
    await user.click(screen.getByRole("button", { name: "Browse all models" }));

    const search = screen.getByPlaceholderText("Search models...");

    expect(search).toHaveAttribute("spellcheck", "false");
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

  it("shows a loading state while models are refreshing", async () => {
    const user = userEvent.setup();

    render(
      <AgentModelPicker
        agents={AGENTS}
        selectedAgentId="goose"
        onAgentChange={vi.fn()}
        currentModelId={null}
        currentModelName={null}
        availableModels={[]}
        modelsLoading
        onModelChange={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    expect(screen.getByText("Loading models...")).toBeInTheDocument();
  });

  it("shows an empty-state message when no inventory models are available", async () => {
    const user = userEvent.setup();

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

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    expect(screen.getByText("No models available")).toBeInTheDocument();
  });
});
