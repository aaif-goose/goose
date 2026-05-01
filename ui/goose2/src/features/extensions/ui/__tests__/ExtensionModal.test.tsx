import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionEntry } from "../../types";
import { ExtensionModal } from "../ExtensionModal";

const extension: ExtensionEntry = {
  type: "stdio",
  name: "github",
  description: "Issue tracker",
  cmd: "npx",
  args: [],
  config_key: "github",
  enabled: true,
};

describe("ExtensionModal", () => {
  it("confirms before deleting an extension", async () => {
    const user = userEvent.setup();
    const handleDelete = vi.fn().mockResolvedValue(undefined);

    render(
      <ExtensionModal
        extension={extension}
        onSubmit={vi.fn()}
        onDelete={handleDelete}
        onClose={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Delete Extension" }));

    expect(handleDelete).not.toHaveBeenCalled();
    const confirmation = screen.getByRole("dialog", {
      name: 'Delete "github" permanently?',
    });
    expect(
      within(confirmation).getByText('Delete "github" permanently?'),
    ).toBeInTheDocument();

    await user.click(
      within(confirmation).getByRole("button", { name: "Delete Extension" }),
    );

    expect(handleDelete).toHaveBeenCalledWith("github");
  });

  it("dismisses the delete confirmation when clicking outside it", async () => {
    const user = userEvent.setup();
    const handleDelete = vi.fn().mockResolvedValue(undefined);
    const handleClose = vi.fn();

    render(
      <ExtensionModal
        extension={extension}
        onSubmit={vi.fn()}
        onDelete={handleDelete}
        onClose={handleClose}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Delete Extension" }));
    expect(
      screen.getByRole("dialog", {
        name: 'Delete "github" permanently?',
      }),
    ).toBeInTheDocument();

    const overlays = document.querySelectorAll('[data-slot$="dialog-overlay"]');
    expect(overlays).toHaveLength(2);
    expect(overlays[overlays.length - 1]).toHaveClass("z-[70]");
    await user.click(overlays[overlays.length - 1] as HTMLElement);

    await waitFor(() => {
      expect(
        screen.queryByRole("dialog", {
          name: 'Delete "github" permanently?',
        }),
      ).not.toBeInTheDocument();
    });
    expect(handleDelete).not.toHaveBeenCalled();
    expect(handleClose).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: "Edit Extension" }),
    ).toBeInTheDocument();
  });
});
