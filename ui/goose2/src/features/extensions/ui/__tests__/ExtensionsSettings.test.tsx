import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExtensionEntry } from "../../types";
import { ExtensionsSettings } from "../ExtensionsSettings";

const mockUseExtensionsSettings = vi.fn();

vi.mock("@/features/extensions/hooks/useExtensionsSettings", () => ({
  useExtensionsSettings: () => mockUseExtensionsSettings(),
}));

const extensions: ExtensionEntry[] = [
  {
    type: "stdio",
    name: "github",
    description: "Issue tracker",
    cmd: "npx",
    args: [],
    config_key: "github",
    enabled: true,
  },
  {
    type: "builtin",
    name: "developer",
    display_name: "Developer",
    description: "Code tools",
    config_key: "developer",
    enabled: true,
  },
  {
    type: "platform",
    name: "summarize",
    display_name: "Summarize",
    description: "Summarize files",
    config_key: "summarize",
    enabled: false,
  },
];

describe("ExtensionsSettings", () => {
  beforeEach(() => {
    mockUseExtensionsSettings.mockReturnValue({
      extensions,
      isLoading: false,
      modalMode: null,
      editingExtension: null,
      handleAdd: vi.fn(),
      handleConfigure: vi.fn(),
      handleSubmit: vi.fn(),
      handleDelete: vi.fn(),
      handleModalClose: vi.fn(),
    });
  });

  it("reveals matching Goose capabilities while searching", async () => {
    const user = userEvent.setup();
    render(<ExtensionsSettings />);

    expect(screen.queryByText("Developer")).not.toBeInTheDocument();

    await user.type(screen.getByRole("searchbox"), "developer");

    expect(screen.getByText("Developer")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: /show .*built-in goose capabilities/i,
      }),
    ).not.toBeInTheDocument();
  });

  it("does not show global enable toggles", async () => {
    const user = userEvent.setup();
    render(<ExtensionsSettings />);

    expect(
      screen.queryByRole("switch", { name: /disable github/i }),
    ).not.toBeInTheDocument();

    await user.type(screen.getByRole("searchbox"), "summarize");

    expect(
      screen.queryByRole("switch", { name: /enable summarize/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("switch", { name: /enable developer/i }),
    ).not.toBeInTheDocument();
  });
});
