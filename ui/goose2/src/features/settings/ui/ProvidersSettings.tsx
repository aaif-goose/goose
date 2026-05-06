import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, buttonVariants } from "@/shared/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Separator } from "@/shared/ui/separator";
import { Spinner } from "@/shared/ui/spinner";
import { IconChevronDown, IconPlus } from "@tabler/icons-react";
import {
  getAgentProvidersFromEntries,
  getModelProvidersFromEntries,
} from "@/features/providers/providerCatalog";
import { useCredentials } from "@/features/providers/hooks/useCredentials";
import { useDistroStore } from "@/features/settings/stores/distroStore";
import { filterModelProvidersForDistro } from "@/features/providers/distroProviderConstraints";
import { useCustomProviders } from "@/features/providers/hooks/useCustomProviders";
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
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { AgentProviderCard } from "./AgentProviderCard";
import { ModelProviderRow } from "./ModelProviderRow";
import { SettingsPage } from "@/shared/ui/SettingsPage";
import {
  catalogEntryToTemplate,
  formValueToDraft,
  readResponseToFormValue,
  templateToFormValue,
} from "./customProviderFormAdapters";
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

interface PendingCustomProviderDelete {
  providerId: string;
  displayName: string;
  resolve: (deleted: boolean) => void;
  reject: (error: unknown) => void;
}

export function ProvidersSettings() {
  const { t } = useTranslation(["settings", "common"]);
  const distro = useDistroStore((state) => state.manifest);
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
  const [customProviderDeleteError, setCustomProviderDeleteError] =
    useState("");
  const [pendingCustomProviderDelete, setPendingCustomProviderDelete] =
    useState<PendingCustomProviderDelete | null>(null);
  const inventoryEntries = useProviderInventoryStore((state) => state.entries);
  const catalogEntries = useProviderCatalogStore((state) => state.entries);
  const catalogLoading = useProviderCatalogStore((state) => state.loading);
  const catalogLoaded = useProviderCatalogStore((state) => state.loaded);
  const catalogError = useProviderCatalogStore((state) => state.error);
  const loadCatalog = useProviderCatalogStore((state) => state.load);

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
    () =>
      toDisplayInfo(
        getAgentProvidersFromEntries(catalogEntries),
        configuredIds,
      ),
    [configuredIds, catalogEntries],
  );

  const allModels = useMemo(
    () =>
      toDisplayInfo(
        filterModelProvidersForDistro(
          getModelProvidersFromEntries(catalogEntries),
          distro,
        ),
        configuredIds,
      ),
    [configuredIds, distro, catalogEntries],
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
    if (!loading && catalogLoaded && modelOrder === null) {
      setModelOrder(sortedModels.map((model) => model.id));
    }
  }, [loading, catalogLoaded, modelOrder, sortedModels]);

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

  const defaultModels = orderedModels.filter((m) => m.group === "default");
  const additionalModels = orderedModels.filter(
    (m) => m.group === "additional",
  );
  const visibleModels = showAllModels ? orderedModels : defaultModels;

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
      setCustomProviderError("");
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
    } catch (error) {
      setCustomProviderTemplates([]);
      setCustomProviderError(
        error instanceof Error
          ? error.message
          : t("providers.custom.errors.templatesFailed"),
      );
    }
  }

  async function openCreateCustomProvider() {
    setCustomProviderError("");
    setCustomProviderDeleteError("");
    setCustomDialogMode("create");
    setCustomProviderDraft(null);
    setCustomDialogOpen(true);
    await loadTemplates();
  }

  async function openEditCustomProvider(providerId: string) {
    setCustomProviderError("");
    setCustomProviderDeleteError("");
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
    const providerName =
      customProviders.find((provider) => provider.providerId === providerId)
        ?.displayName ?? providerId;

    return new Promise<boolean>((resolve, reject) => {
      setPendingCustomProviderDelete({
        providerId,
        displayName: providerName,
        resolve,
        reject,
      });
    });
  }

  function cancelCustomProviderDelete() {
    pendingCustomProviderDelete?.resolve(false);
    setPendingCustomProviderDelete(null);
  }

  async function confirmCustomProviderDelete() {
    const pendingDelete = pendingCustomProviderDelete;
    if (!pendingDelete) {
      return;
    }

    setCustomProviderDeleteError("");
    try {
      await customProvidersApi.remove(pendingDelete.providerId);
      pendingDelete.resolve(true);
      setPendingCustomProviderDelete(null);
    } catch (error) {
      setCustomProviderDeleteError(
        error instanceof Error
          ? error.message
          : t("providers.custom.errors.deleteFailed"),
      );
      pendingDelete.reject(error);
      setPendingCustomProviderDelete(null);
    }
  }

  return (
    <SettingsPage
      title={t("providers.title")}
      actions={
        <Button
          type="button"
          variant="outline"
          size="xxs"
          onClick={() => void openCreateCustomProvider()}
          leftIcon={<IconPlus />}
          className="shrink-0"
        >
          {t("providers.custom.addButton")}
        </Button>
      }
    >
      {catalogError && (
        <div
          role="alert"
          className="mb-4 rounded-md border border-danger/30 bg-danger/10 p-3"
        >
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs text-danger">{catalogError}</p>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={() => void loadCatalog()}
            >
              {t("common:actions.retry")}
            </Button>
          </div>
        </div>
      )}

      {catalogLoading && (
        <div
          role="status"
          className="mb-4 flex items-center gap-2 text-xs text-muted-foreground"
        >
          <Spinner className="size-3.5" />
          {t("providers.catalog.loading")}
        </div>
      )}

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
          <div className="flex items-center gap-2">
            <h4 className="text-sm font-semibold">
              {t("providers.models.title")}
            </h4>
            {loading ? (
              <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                <Spinner className="size-3 text-brand" />
                {t("providers.models.checkingStatus")}
              </span>
            ) : null}
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("providers.models.description")}
          </p>
        </div>

        {customProviderError ? (
          <p
            role="alert"
            className="mb-3 rounded-md border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger"
          >
            {customProviderError}
          </p>
        ) : null}
        {customProviderDeleteError ? (
          <p
            role="alert"
            className="mb-3 rounded-md border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger"
          >
            {customProviderDeleteError}
          </p>
        ) : null}

        {customProviders.length > 0 ? (
          <div className="mb-3 space-y-2">
            {customProviders.map((provider) => (
              <CustomProviderChoice
                key={provider.providerId}
                provider={provider}
                onEdit={() => void openEditCustomProvider(provider.providerId)}
                onDelete={() =>
                  void deleteCustomProvider(provider.providerId).catch(() => {})
                }
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

        {!showAllModels && additionalModels.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setShowAllModels(true)}
            className="mt-2 w-full text-muted-foreground"
          >
            {t("providers.showMore", { count: additionalModels.length })}
            <IconChevronDown className="size-3" />
          </Button>
        )}

        {showAllModels && additionalModels.length > 0 && (
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

      <AlertDialog
        open={!!pendingCustomProviderDelete}
        onOpenChange={(open) => {
          if (!open) {
            cancelCustomProviderDelete();
          }
        }}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("providers.custom.confirmDeleteTitle", {
                name: pendingCustomProviderDelete?.displayName ?? "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("providers.custom.confirmDelete", {
                name: pendingCustomProviderDelete?.displayName ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={(event) => {
                event.preventDefault();
                void confirmCustomProviderDelete();
              }}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SettingsPage>
  );
}
