import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GooseAutoCompactSettings } from "../GooseAutoCompactSettings";

const mockSetAutoCompactThreshold = vi.fn();
const mockUseAutoCompactPreferences = vi.fn();

vi.mock("@/shared/i18n", async () => {
  const actual =
    await vi.importActual<typeof import("@/shared/i18n")>("@/shared/i18n");

  return {
    ...actual,
    useLocaleFormatting: () => ({
      formatNumber: (value: number, options?: Intl.NumberFormatOptions) =>
        new Intl.NumberFormat("en-US", options).format(value),
    }),
  };
});

vi.mock("@/features/chat/hooks/useAutoCompactPreferences", () => ({
  useAutoCompactPreferences: () => mockUseAutoCompactPreferences(),
}));

describe("GooseAutoCompactSettings", () => {
  beforeEach(() => {
    mockSetAutoCompactThreshold.mockReset();
    mockSetAutoCompactThreshold.mockResolvedValue(undefined);
    mockUseAutoCompactPreferences.mockReset();
    mockUseAutoCompactPreferences.mockReturnValue({
      autoCompactThreshold: 0.8,
      isHydrated: true,
      setAutoCompactThreshold: mockSetAutoCompactThreshold,
    });
  });

  it("updates the auto-compaction threshold from the slider", async () => {
    const user = userEvent.setup();

    render(<GooseAutoCompactSettings />);

    const slider = screen.getByRole("slider", {
      name: /auto-compact context/i,
    });
    slider.focus();
    await user.keyboard("{ArrowRight}");

    await waitFor(() =>
      expect(mockSetAutoCompactThreshold).toHaveBeenCalledWith(0.81),
    );
  });

  it("turns auto-compaction off from the toggle and disables the slider", async () => {
    const user = userEvent.setup();

    render(<GooseAutoCompactSettings />);

    await user.click(
      screen.getByRole("switch", {
        name: /toggle auto-compact context/i,
      }),
    );

    await waitFor(() =>
      expect(mockSetAutoCompactThreshold).toHaveBeenCalledWith(1),
    );

    expect(
      screen.getByRole("slider", { name: /auto-compact context/i }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(screen.getAllByText("Off")).toHaveLength(2);
  });
});
