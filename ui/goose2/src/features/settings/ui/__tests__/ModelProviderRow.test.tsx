import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ComponentType } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getModelProviders } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import type { ProviderCatalogEntry } from "@/shared/types/providers";
import { ModelProviderRow } from "../ModelProviderRow";

const Row = ModelProviderRow as unknown as ComponentType<
  Record<string, unknown>
>;

function modelProvider(id: string, status: "connected" | "not_configured") {
  const provider = getModelProviders().find((entry) => entry.id === id);
  if (!provider) {
    throw new Error(`missing provider fixture: ${id}`);
  }
  return {
    ...provider,
    status,
  };
}

const providerCatalog: ProviderCatalogEntry[] = [
  {
    id: "databricks",
    displayName: "Databricks",
    category: "model",
    description: "Databricks Foundation Models",
    setupMethod: "host_with_oauth_fallback",
    fields: [
      {
        key: "DATABRICKS_HOST",
        label: "Host URL",
        secret: false,
        required: true,
        placeholder: "https://dbc-...cloud.databricks.com",
      },
      {
        key: "DATABRICKS_TOKEN",
        label: "Access Token",
        secret: true,
        required: false,
        placeholder: "Paste your access token",
      },
    ],
    group: "default",
  },
  {
    id: "ollama",
    displayName: "Ollama",
    category: "model",
    description: "Run local or self-hosted models",
    setupMethod: "config_fields",
    fields: [
      {
        key: "OLLAMA_HOST",
        label: "Host",
        secret: false,
        required: true,
        placeholder: "localhost or http://localhost:11434",
        defaultValue: "http://localhost:11434",
      },
    ],
    group: "default",
  },
  {
    id: "anthropic",
    displayName: "Anthropic",
    category: "model",
    description: "Claude models",
    setupMethod: "single_api_key",
    group: "default",
  },
  {
    id: "google",
    displayName: "Google Gemini",
    category: "model",
    description: "Gemini models",
    setupMethod: "single_api_key",
    fields: [
      {
        key: "GOOGLE_API_KEY",
        label: "API Key",
        secret: true,
        required: true,
        placeholder: "Paste your API key",
      },
    ],
    group: "default",
  },
];

describe("ModelProviderRow", () => {
  const onGetConfig = vi.fn();
  const onSaveFields = vi.fn();
  const onRemoveConfig = vi.fn();
  const onCompleteNativeSetup = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    useProviderCatalogStore.getState().setEntries(providerCatalog);
    onGetConfig.mockResolvedValue([]);
    onSaveFields.mockResolvedValue(undefined);
    onRemoveConfig.mockResolvedValue(undefined);
    onCompleteNativeSetup.mockResolvedValue(undefined);
  });

  it("saves all changed setup fields from one setup submit", async () => {
    const user = userEvent.setup();

    render(
      <ModelProviderRow
        provider={modelProvider("databricks", "not_configured")}
        onGetConfig={onGetConfig}
        onSaveFields={onSaveFields}
        onRemoveConfig={onRemoveConfig}
        onCompleteNativeSetup={onCompleteNativeSetup}
      />,
    );

    await user.click(screen.getByRole("button", { name: /databricks/i }));
    await user.type(
      await screen.findByPlaceholderText(/cloud\.databricks\.com/i),
      "https://dbc-test.cloud.databricks.com",
    );
    await user.type(
      screen.getByPlaceholderText(/paste your access token/i),
      "databricks-token",
    );
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(onSaveFields).toHaveBeenCalledTimes(1));
    expect(onSaveFields).toHaveBeenCalledWith([
      {
        key: "DATABRICKS_HOST",
        value: "https://dbc-test.cloud.databricks.com",
        isSecret: false,
      },
      {
        key: "DATABRICKS_TOKEN",
        value: "databricks-token",
        isSecret: true,
      },
    ]);
  });

  it("pre-fills and saves provider field defaults", async () => {
    const user = userEvent.setup();

    render(
      <ModelProviderRow
        provider={modelProvider("ollama", "not_configured")}
        onGetConfig={onGetConfig}
        onSaveFields={onSaveFields}
        onRemoveConfig={onRemoveConfig}
        onCompleteNativeSetup={onCompleteNativeSetup}
      />,
    );

    await user.click(screen.getByRole("button", { name: /ollama/i }));

    expect(
      await screen.findByDisplayValue("http://localhost:11434"),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(onSaveFields).toHaveBeenCalledTimes(1));
    expect(onSaveFields).toHaveBeenCalledWith([
      {
        key: "OLLAMA_HOST",
        value: "http://localhost:11434",
        isSecret: false,
      },
    ]);
  });

  it("shows the connected row while model inventory is still loading", async () => {
    const user = userEvent.setup();

    render(
      <Row
        provider={modelProvider("anthropic", "connected")}
        onGetConfig={onGetConfig}
        onSaveFields={onSaveFields}
        onRemoveConfig={onRemoveConfig}
        onCompleteNativeSetup={onCompleteNativeSetup}
        inventorySyncing={true}
      />,
    );

    await user.click(screen.getByRole("button", { name: /anthropic/i }));

    expect(screen.getByText(/loading models/i)).toBeInTheDocument();
  });

  it("shows a non-blocking inventory warning without replacing the connected state", async () => {
    const user = userEvent.setup();

    render(
      <Row
        provider={modelProvider("anthropic", "connected")}
        onGetConfig={onGetConfig}
        onSaveFields={onSaveFields}
        onRemoveConfig={onRemoveConfig}
        onCompleteNativeSetup={onCompleteNativeSetup}
        inventoryWarning="Model refresh failed"
      />,
    );

    await user.click(screen.getByRole("button", { name: /anthropic/i }));

    expect(screen.getByText(/model refresh failed/i)).toBeInTheDocument();
  });

  it("switches from setup save to connected controls after first configuration", async () => {
    const user = userEvent.setup();
    let saved = false;

    function SetupSaveRow() {
      const [status, setStatus] = useState<"connected" | "not_configured">(
        "not_configured",
      );

      return (
        <ModelProviderRow
          provider={modelProvider("google", status)}
          onGetConfig={async () =>
            saved
              ? [
                  {
                    key: "GOOGLE_API_KEY",
                    value: null,
                    isSet: true,
                    isSecret: true,
                    required: true,
                  },
                ]
              : []
          }
          onSaveFields={async () => {
            saved = true;
            setStatus("connected");
          }}
          onRemoveConfig={onRemoveConfig}
          onCompleteNativeSetup={onCompleteNativeSetup}
        />
      );
    }

    render(<SetupSaveRow />);

    await user.click(screen.getByRole("button", { name: /google gemini/i }));
    await user.type(
      await screen.findByPlaceholderText(/paste your api key/i),
      "google-token",
    );
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /disconnect/i }),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: /edit/i })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /saved/i }),
    ).not.toBeInTheDocument();
  });
});
