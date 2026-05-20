import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { renderWithProviders } from "@/test/render";
import { GeneralSettings } from "../GeneralSettings";

describe("GeneralSettings appearance section", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-density");
  });

  it("updates interface density from the appearance controls", async () => {
    const user = userEvent.setup();

    renderWithProviders(
      <ThemeProvider>
        <GeneralSettings />
      </ThemeProvider>,
    );

    const compact = screen.getByRole("radio", { name: "Compact" });

    await user.click(compact);

    await waitFor(() => {
      expect(localStorage.getItem("goose-density")).toBe("compact");
      expect(document.documentElement.dataset.density).toBe("compact");
    });
    expect(compact).toHaveAttribute("data-state", "on");
  });
});
