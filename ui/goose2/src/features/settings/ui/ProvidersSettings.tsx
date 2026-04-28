import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { Spinner } from "@/shared/ui/spinner";
import { IconChevronDown } from "@tabler/icons-react";
import {
  getAgentProviders,
  getModelProviders,
} from "@/features/providers/providerCatalog";
import { useCredentials } from "@/features/providers/hooks/useCredentials";
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

export function ProvidersSettings() {
  const { t } = useTranslation(["settings", "common"]);
  const [showAllModels, setShowAllModels] = useState(false);
  const [modelOrder, setModelOrder] = useState<string[] | null>(null);

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

  return (
    <div>
      <h3 className="text-lg font-semibold font-display tracking-tight">
        {t("providers.title")}
      </h3>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("providers.description")}
      </p>

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
    </div>
  );
}
