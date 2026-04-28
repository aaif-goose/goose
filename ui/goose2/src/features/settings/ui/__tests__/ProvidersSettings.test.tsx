import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProvidersSettings } from "../ProvidersSettings";

const mocks = vi.hoisted(() => ({
  useCredentials: vi.fn(),
}));

vi.mock("@/features/providers/hooks/useCredentials", () => ({
  useCredentials: () => mocks.useCredentials(),
}));

describe("ProvidersSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCredentials.mockReturnValue({
      configuredIds: new Set<string>(),
      loading: false,
      saving: false,
      savingProviderIds: new Set<string>(),
      syncingProviderIds: new Set<string>(),
      inventoryWarnings: new Map<string, string>(),
      getConfig: vi.fn(),
      save: vi.fn(),
      remove: vi.fn(),
      completeNativeSetup: vi.fn(),
    });
  });

  it("does not show the restart banner for provider credential changes", () => {
    render(<ProvidersSettings />);

    expect(
      screen.queryByText(/restart to apply credential changes/i),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /restart now/i }),
    ).not.toBeInTheDocument();
  });
});
