import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { ProvidersSettings } from "../ProvidersSettings";

const mocks = vi.hoisted(() => ({
  useCredentials: vi.fn(),
}));

vi.mock("@/features/providers/hooks/useCredentials", () => ({
  useCredentials: () => mocks.useCredentials(),
}));

function providerEntry(
  overrides: Partial<ProviderInventoryEntryDto>,
): ProviderInventoryEntryDto {
  return {
    providerId: "custom_openai",
    providerName: "Custom OpenAI",
    description: "",
    defaultModel: "",
    configured: true,
    providerType: "Custom",
    configKeys: [],
    setupSteps: [],
    supportsRefresh: true,
    refreshing: false,
    models: [],
    stale: false,
    ...overrides,
  };
}

describe("ProvidersSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useProviderInventoryStore.getState().setEntries([]);
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

  it("renders the static provider catalog while credential status is loading", () => {
    mocks.useCredentials.mockReturnValue({
      configuredIds: new Set<string>(),
      loading: true,
      saving: false,
      savingProviderIds: new Set<string>(),
      syncingProviderIds: new Set<string>(),
      inventoryWarnings: new Map<string, string>(),
      getConfig: vi.fn(),
      save: vi.fn(),
      remove: vi.fn(),
      completeNativeSetup: vi.fn(),
    });

    render(<ProvidersSettings />);

    expect(screen.getByText("Providers")).toBeInTheDocument();
    expect(screen.getByText("Anthropic")).toBeInTheDocument();
    expect(screen.getByText("Checking provider status...")).toBeInTheDocument();
  });

  it("matches main by ordering connected model providers first after status loads", () => {
    mocks.useCredentials.mockReturnValue({
      configuredIds: new Set<string>(["openai", "databricks"]),
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

    render(<ProvidersSettings />);

    const openai = screen.getByText("OpenAI");
    const databricks = screen.getByText("Databricks");
    const anthropic = screen.getByText("Anthropic");

    expect(
      openai.compareDocumentPosition(databricks) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      databricks.compareDocumentPosition(anthropic) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("shows the custom provider entry point near model providers", async () => {
    const user = userEvent.setup();
    render(<ProvidersSettings />);

    await user.click(
      screen.getByRole("button", { name: /add custom provider/i }),
    );

    expect(
      screen.getByRole("dialog", { name: /add custom provider/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/manual setup/i)).toBeInTheDocument();
  });

  it("shows custom inventory providers with edit and delete actions", () => {
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "custom_acme",
        providerName: "Acme Models",
        models: [
          {
            id: "acme-fast",
            name: "acme-fast",
          },
        ],
      }),
    ]);

    render(<ProvidersSettings />);

    expect(screen.getByText("Acme Models")).toBeInTheDocument();
    expect(screen.getByText("1 model")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /edit acme models/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete acme models/i }),
    ).toBeInTheDocument();
  });
});
