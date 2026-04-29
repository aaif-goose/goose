import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { Spinner } from "@/shared/ui/spinner";
import { IconChevronDown, IconPlus } from "@tabler/icons-react";
import {
  getAgentProviders,
  getModelProviders,
} from "@/features/providers/providerCatalog";
import { useCredentials } from "@/features/providers/hooks/useCredentials";
import { useCustomProviders } from "@/features/providers/hooks/useCustomProviders";
import { normalizeCustomProviderEngine } from "@/features/providers/lib/customProviderDraft";
import {
  formatCustomProviderModels,
  parseCustomProviderModels,
} from "@/features/providers/lib/customProviderModels";
import type {
  CustomProviderDraft,
  CustomProviderEngine,
  CustomProviderReadResponse,
  ProviderCatalogEntryDto,
  ProviderTemplateDto,
} from "@/features/providers/lib/customProviderTypes";
import {
  CustomProviderChoice,
  type CustomProviderChoiceInfo,
} from "@/features/providers/ui/CustomProviderChoice";
import {
  CustomProviderDialog,
  type CustomProviderMutationInput,
} from "@/features/providers/ui/CustomProviderDialog";
import type {
  CustomProviderFormValues,
  ProviderTemplate,
} from "@/features/providers/ui/CustomProviderForm";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { AgentProviderCard } from "./AgentProviderCard";
import { ModelProviderRow } from "./ModelProviderRow";
import type {
  ProviderDisplayInfo,
  ProviderSetupStatus,
  ProviderCatalogEntry,
} from "@/shared/types/providers";

function resolveStatus(
  entry: ProviderCatalogEntry,
  configuredIds: Set<string>,
): ProviderSetupStatus {
  if (entry.id === "goose") return "built_in";
  if (entry.category === "agent") return "not_installed";
  if (configuredIds.has(entry.id)) return "connected";
  return "not_configured";
}

function toDisplayInfo(
  entries: ProviderCatalogEntry[],
  configuredIds: Set<string>,
): ProviderDisplayInfo[] {
  return entries.map((entry) => ({
    ...entry,
    status: resolveStatus(entry, configuredIds),
  }));
}

function isCustomProviderEntry(entry: {
  providerId: string;
  providerType?: string;
}) {
  return entry.providerType === "Custom";
}

function toCustomProviderChoiceInfo(entry: {
  providerId: string;
  providerName: string;
  description?: string;
  configured: boolean;
  models: unknown[];
}): CustomProviderChoiceInfo {
  return {
    providerId: entry.providerId,
    displayName: entry.providerName,
    description: entry.description || undefined,
    configured: entry.configured,
    modelCount: entry.models.length,
  };
}

function engineForCustomProviderFormat(format: string): CustomProviderEngine {
  if (format === "anthropic") {
    return "anthropic_compatible";
  }
  if (format === "ollama") {
    return "ollama_compatible";
  }
  return "openai_compatible";
}

function templateToFormValue(template: ProviderTemplateDto): ProviderTemplate {
  const models = (template.models ?? [])
    .filter((model) => !model.deprecated)
    .map((model) => model.id);

  return {
    id: template.providerId,
    displayName: template.name,
    engine: engineForCustomProviderFormat(template.format),
    apiUrl: template.apiUrl,
    requiresAuth: true,
    supportsStreaming: template.supportsStreaming,
    models,
    headers: [],
  };
}

function catalogEntryToTemplate(
  entry: ProviderCatalogEntryDto,
): ProviderTemplate {
  return {
    id: entry.providerId,
    displayName: entry.name,
    engine: engineForCustomProviderFormat(entry.format),
    apiUrl: entry.apiUrl,
    requiresAuth: true,
    supportsStreaming: true,
    models: [],
    headers: [],
  };
}

function readResponseToFormValue(
  response: CustomProviderReadResponse,
): CustomProviderFormValues {
  const provider = response.provider;
  return {
    providerId: provider.providerId,
    displayName: provider.displayName,
    engine: normalizeCustomProviderEngine(provider.engine),
    apiUrl: provider.apiUrl,
    basePath: provider.basePath ?? "",
    requiresAuth: provider.requiresAuth,
    apiKey: "",
    models: parseCustomProviderModels(provider.models ?? []),
    supportsStreaming: provider.supportsStreaming ?? true,
    headers: Object.entries(provider.headers ?? {}).map(([key, value]) => ({
      key,
      value,
    })),
    catalogProviderId: provider.catalogProviderId ?? undefined,
  };
}

function formValueToDraft(
  input: CustomProviderMutationInput,
): CustomProviderDraft {
  const models = parseCustomProviderModels(input.models);
  return {
    providerId: input.providerId,
    editable: true,
    engine: input.engine,
    displayName: input.displayName,
    apiUrl: input.apiUrl,
    basePath: input.basePath,
    apiKey: input.apiKey,
    modelsInput: formatCustomProviderModels(models),
    models,
    requiresAuth: input.requiresAuth,
    supportsStreaming: input.supportsStreaming,
    headers: input.headers,
    catalogProviderId: input.catalogProviderId,
  };
}

export function ProvidersSettings() {
  const { t } = useTranslation(["settings", "common"]);
  const [showAllModels, setShowAllModels] = useState(false);
  const [modelOrder, setModelOrder] = useState<string[] | null>(null);
  const [customDialogOpen, setCustomDialogOpen] = useState(false);
  const [customDialogMode, setCustomDialogMode] = useState<"create" | "edit">(
    "create",
  );
  const [customProviderDraft, setCustomProviderDraft] =
    useState<CustomProviderFormValues | null>(null);
  const [customProviderTemplates, setCustomProviderTemplates] = useState<
    ProviderTemplate[]
  >([]);
  const [customProviderError, setCustomProviderError] = useState("");
  const inventoryEntries = useProviderInventoryStore((state) => state.entries);

  const {
    configuredIds,
    loading,
    savingProviderIds,
    syncingProviderIds,
    inventoryWarnings,
    getConfig,
    save,
    remove,
    completeNativeSetup,
  } = useCredentials();
  const customProvidersApi = useCustomProviders();

  const agents = useMemo(
    () => toDisplayInfo(getAgentProviders(), configuredIds),
    [configuredIds],
  );

  const allModels = useMemo(
    () => toDisplayInfo(getModelProviders(), configuredIds),
    [configuredIds],
  );

  const sortedModels = useMemo(() => {
    return [...allModels].sort((a, b) => {
      const connected = (p: ProviderDisplayInfo) =>
        p.status === "connected" || p.status === "built_in";
      if (connected(a) && !connected(b)) return -1;
      if (!connected(a) && connected(b)) return 1;
      return 0;
    });
  }, [allModels]);

  useEffect(() => {
    if (!loading && modelOrder === null) {
      setModelOrder(sortedModels.map((model) => model.id));
    }
  }, [loading, modelOrder, sortedModels]);

  const orderedModels = useMemo(() => {
    if (!modelOrder) {
      return sortedModels;
    }

    const orderIndex = new Map(
      modelOrder.map((modelId, index) => [modelId, index]),
    );

    return [...allModels].sort((a, b) => {
      const aIndex = orderIndex.get(a.id);
      const bIndex = orderIndex.get(b.id);

      if (aIndex !== undefined && bIndex !== undefined) {
        return aIndex - bIndex;
      }
      if (aIndex !== undefined) {
        return -1;
      }
      if (bIndex !== undefined) {
        return 1;
      }
      return a.displayName.localeCompare(b.displayName);
    });
  }, [allModels, modelOrder, sortedModels]);

  const promotedModels = orderedModels.filter(
    (m) => m.tier === "promoted" || m.tier === "standard",
  );
  const advancedModels = orderedModels.filter((m) => m.tier === "advanced");
  const visibleModels = showAllModels ? orderedModels : promotedModels;

  const customProviders = useMemo(
    () =>
      [...inventoryEntries.values()]
        .filter(isCustomProviderEntry)
        .map(toCustomProviderChoiceInfo)
        .sort((a, b) => a.displayName.localeCompare(b.displayName)),
    [inventoryEntries],
  );

  async function loadTemplates() {
    try {
      const catalog = await customProvidersApi.loadCatalog();
      const templates = await Promise.all(
        catalog.map(async (entry) => {
          try {
            return templateToFormValue(
              await customProvidersApi.getTemplate(entry.providerId),
            );
          } catch {
            return catalogEntryToTemplate(entry);
          }
        }),
      );
      setCustomProviderTemplates(templates);
    } catch {
      setCustomProviderTemplates([]);
    }
  }

  async function openCreateCustomProvider() {
    setCustomProviderError("");
    setCustomDialogMode("create");
    setCustomProviderDraft(null);
    setCustomDialogOpen(true);
    await loadTemplates();
  }

  async function openEditCustomProvider(providerId: string) {
    setCustomProviderError("");
    try {
      const provider = readResponseToFormValue(
        await customProvidersApi.read(providerId),
      );
      setCustomDialogMode("edit");
      setCustomProviderDraft(provider);
      setCustomDialogOpen(true);
      await loadTemplates();
    } catch (error) {
      setCustomProviderError(
        error instanceof Error
          ? error.message
          : t("providers.custom.errors.loadFailed"),
      );
    }
  }

  async function createCustomProvider(input: CustomProviderMutationInput) {
    await customProvidersApi.saveDraft(formValueToDraft(input));
  }

  async function updateCustomProvider(
    providerId: string,
    input: CustomProviderMutationInput,
  ) {
    await customProvidersApi.saveDraft(formValueToDraft(input), { providerId });
  }

  async function deleteCustomProvider(providerId: string) {
    await customProvidersApi.remove(providerId);
  }

  return (
    <div>
      <h3 className="text-lg font-semibold font-display tracking-tight">
        {t("providers.title")}
      </h3>

      <Separator className="my-4" />

      <section>
        <div className="mb-3">
          <h4 className="text-sm font-semibold">
            {t("providers.agents.title")}
          </h4>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("providers.agents.description")}
          </p>
        </div>

        <div className="grid grid-cols-2 gap-3">
          {agents.map((agent) => (
            <AgentProviderCard key={agent.id} provider={agent} />
          ))}
        </div>
      </section>

      <Separator className="my-6" />

      <section>
        <div className="mb-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                <h4 className="text-sm font-semibold">
                  {t("providers.models.title")}
                </h4>
                {loading ? (
                  <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                    <Spinner className="size-3 text-accent" />
                    {t("providers.models.checkingStatus")}
                  </span>
                ) : null}
              </div>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("providers.models.description")}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void openCreateCustomProvider()}
              leftIcon={<IconPlus />}
              className="shrink-0"
            >
              {t("providers.custom.addButton")}
            </Button>
          </div>
        </div>

        {customProviderError ? (
          <p className="mb-3 rounded-md border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger">
            {customProviderError}
          </p>
        ) : null}

        {customProviders.length > 0 ? (
          <div className="mb-3 space-y-2">
            {customProviders.map((provider) => (
              <CustomProviderChoice
                key={provider.providerId}
                provider={provider}
                onEdit={() => void openEditCustomProvider(provider.providerId)}
                onDelete={() => void deleteCustomProvider(provider.providerId)}
                deleting={customProvidersApi.deletingProviderIds.has(
                  provider.providerId,
                )}
              />
            ))}
          </div>
        ) : null}

        <div className="space-y-2">
          {visibleModels.map((model) => (
            <ModelProviderRow
              key={model.id}
              provider={model}
              onGetConfig={getConfig}
              onSaveFields={(fields) => save(model.id, fields)}
              onRemoveConfig={() => remove(model.id)}
              onCompleteNativeSetup={completeNativeSetup}
              saving={savingProviderIds.has(model.id)}
              inventorySyncing={syncingProviderIds.has(model.id)}
              inventoryWarning={inventoryWarnings.get(model.id)}
            />
          ))}
        </div>

        {!showAllModels && advancedModels.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setShowAllModels(true)}
            className="mt-2 w-full text-muted-foreground"
          >
            {t("providers.showMore", { count: advancedModels.length })}
            <IconChevronDown className="size-3" />
          </Button>
        )}

        {showAllModels && advancedModels.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setShowAllModels(false)}
            className="mt-2 w-full text-muted-foreground"
          >
            {t("providers.showFewer")}
          </Button>
        )}
      </section>

      <CustomProviderDialog
        open={customDialogOpen}
        mode={customDialogMode}
        provider={customProviderDraft}
        templates={customProviderTemplates}
        onOpenChange={setCustomDialogOpen}
        onCreate={createCustomProvider}
        onUpdate={updateCustomProvider}
        onDelete={deleteCustomProvider}
      />
    </div>
  );
}
