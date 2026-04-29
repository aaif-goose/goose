import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useHomeWidgetStore } from "../stores/homeWidgetStore";
import { HomeView } from "./HomeView";

describe("HomeView", () => {
  beforeEach(() => {
    localStorage.clear();
    useHomeWidgetStore.setState({ instances: [] });
    useAgentStore.setState({ personas: [] });
  });

  it("opens the picker from empty canvas and adds a widget", async () => {
    const user = userEvent.setup();
    const { container } = render(<HomeView />);
    const canvas = container.querySelector(".bg-dot-grid");

    expect(canvas).not.toBeNull();
    fireEvent.doubleClick(canvas as Element, { clientX: 100, clientY: 100 });

    await user.click(screen.getByRole("button", { name: /clock/i }));

    expect(useHomeWidgetStore.getState().instances).toMatchObject([
      { type: "clock" },
    ]);
  });

  it("does not open the picker when double-clicking a widget", () => {
    useHomeWidgetStore.getState().addWidget("clock", 130, 66);

    render(<HomeView onOpenAgent={vi.fn()} onSelectSession={vi.fn()} />);
    fireEvent.doubleClick(screen.getByText(/local time/i));

    expect(screen.queryByText("Tile")).not.toBeInTheDocument();
  });

  it("starts a chat from an intentional agent pin click", async () => {
    const onStartChatWithPersona = vi.fn();
    const user = userEvent.setup();
    useAgentStore.setState({
      personas: [
        {
          id: "scout",
          displayName: "Scout",
          systemPrompt: "",
          isBuiltin: true,
          createdAt: "2026-04-29T00:00:00.000Z",
          updatedAt: "2026-04-29T00:00:00.000Z",
        },
      ],
    });
    useHomeWidgetStore.setState({
      instances: [
        {
          id: "agent-pin",
          type: "agentPin",
          x: 0,
          y: 0,
          z: 1,
          state: { agentId: "scout" },
        },
      ],
    });

    render(<HomeView onStartChatWithPersona={onStartChatWithPersona} />);
    expect(screen.queryByText("Start chat")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /scout/i }));

    expect(onStartChatWithPersona).toHaveBeenCalledWith("scout");
  });

  it("does not start a chat when releasing a dragged agent pin", () => {
    const onStartChatWithPersona = vi.fn();
    useAgentStore.setState({
      personas: [
        {
          id: "scout",
          displayName: "Scout",
          systemPrompt: "",
          isBuiltin: true,
          createdAt: "2026-04-29T00:00:00.000Z",
          updatedAt: "2026-04-29T00:00:00.000Z",
        },
      ],
    });
    useHomeWidgetStore.setState({
      instances: [
        {
          id: "agent-pin",
          type: "agentPin",
          x: 0,
          y: 0,
          z: 1,
          state: { agentId: "scout" },
        },
      ],
    });

    render(<HomeView onStartChatWithPersona={onStartChatWithPersona} />);
    const scoutPin = screen.getByRole("button", { name: /scout/i });

    fireEvent.mouseDown(scoutPin, { clientX: 20, clientY: 20 });
    fireEvent.mouseMove(scoutPin, { clientX: 72, clientY: 28 });
    fireEvent.mouseUp(scoutPin, { clientX: 72, clientY: 28 });
    fireEvent.click(scoutPin);

    expect(onStartChatWithPersona).not.toHaveBeenCalled();
  });
});
