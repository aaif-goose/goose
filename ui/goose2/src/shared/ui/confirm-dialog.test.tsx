import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./confirm-dialog";

describe("ConfirmDialog", () => {
  it("routes rejected confirm actions to onConfirmError", async () => {
    const user = userEvent.setup();
    const error = new Error("Delete failed");
    const onConfirm = vi.fn().mockRejectedValue(error);
    const onConfirmError = vi.fn();

    render(
      <ConfirmDialog
        open
        onOpenChange={vi.fn()}
        title="Delete item?"
        description="This cannot be undone."
        cancelLabel="Cancel"
        confirmLabel="Delete"
        onConfirm={onConfirm}
        onConfirmError={onConfirmError}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Delete" }));

    expect(onConfirm).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(onConfirmError).toHaveBeenCalledWith(error);
    });
  });
});
