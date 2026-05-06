import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { renderWithProviders } from "@/test/render";
import { AppearanceSettings } from "../AppearanceSettings";

describe("AppearanceSettings", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-density");
    document.documentElement.style.removeProperty("--density-spacing");
    document.documentElement.style.removeProperty("--spacing");
  });

  it("updates interface density from the appearance controls", async () => {
    const user = userEvent.setup();

    renderWithProviders(
      <ThemeProvider>
        <AppearanceSettings />
      </ThemeProvider>,
    );

    const compact = screen.getByRole("radio", { name: "Compact" });

    await user.click(compact);

    await waitFor(() => {
      expect(localStorage.getItem("goose-density")).toBe("compact");
      expect(document.documentElement.dataset.density).toBe("compact");
      expect(
        document.documentElement.style.getPropertyValue("--density-spacing"),
      ).toBe("");
      expect(document.documentElement.style.getPropertyValue("--spacing")).toBe(
        "",
      );
    });
    expect(compact).toHaveAttribute("data-state", "on");
  });
});
