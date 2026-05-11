import { useState } from "react";
import type { ComponentProps } from "react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import {
  IconArrowRight,
  IconCheck,
  IconPlugConnected,
} from "@tabler/icons-react";
import type { getModelProviders } from "@/features/providers/providerCatalog";
import { ModelProviderRow } from "@/features/settings/ui/ModelProviderRow";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { getProviderIcon } from "@/shared/ui/icons/ProviderIcons";
import { Spinner } from "@/shared/ui/spinner";
import { firstUsableModel } from "../lib/providerDefaults";
import type {
  ProviderFieldSaveInput,
  TFunctionLike,
  UsableDefaultEntry,
} from "../types";

interface ProviderStepProps {
  credentialLoading: boolean;
  modelProviders: ReturnType<typeof getModelProviders>;
  canBrowseAllProviders: boolean;
  usableDefaultEntries: UsableDefaultEntry[];
  configuredIds: Set<string>;
  savingProviderIds: Set<string>;
  syncingProviderIds: Set<string>;
  inventoryWarnings: Map<string, string>;
  selectingProviderId: string | null;
  providerError: string;
  onGetConfig: ComponentProps<typeof ModelProviderRow>["onGetConfig"];
  onSave: (
    providerId: string,
    fields: ProviderFieldSaveInput[],
  ) => Promise<void>;
  onRemove: (providerId: string) => Promise<void>;
  onCompleteNativeSetup: ComponentProps<
    typeof ModelProviderRow
  >["onCompleteNativeSetup"];
  onSelectModelProvider: (entry: ProviderInventoryEntryDto) => void;
  onSelectAgentProvider: (entry: ProviderInventoryEntryDto) => void;
  onBrowseAllProviders: () => void;
  onContinue: () => void;
  t: TFunctionLike;
}

export function ProviderStep({
  credentialLoading,
  modelProviders,
  canBrowseAllProviders,
  usableDefaultEntries,
  configuredIds,
  savingProviderIds,
  syncingProviderIds,
  inventoryWarnings,
  selectingProviderId,
  providerError,
  onGetConfig,
  onSave,
  onRemove,
  onCompleteNativeSetup,
  onSelectModelProvider,
  onSelectAgentProvider,
  onBrowseAllProviders,
  onContinue,
  t,
}: ProviderStepProps) {
  const hasUsableDefaults = usableDefaultEntries.length > 0;
  const [showProviderSetup, setShowProviderSetup] = useState(
    !hasUsableDefaults,
  );

  return (
    <section className="flex w-full flex-col items-center">
      <div
        className={cn(
          "flex size-14 items-center justify-center rounded-[14px]",
          hasUsableDefaults
            ? "bg-green-100/40 text-green-300"
            : "bg-muted text-muted-foreground",
        )}
      >
        {hasUsableDefaults ? (
          <IconCheck className="size-7" strokeWidth={2.25} />
        ) : (
          <IconPlugConnected className="size-6" strokeWidth={1.75} />
        )}
      </div>
      <h2 className="mt-6 text-[22px] font-semibold tracking-tight text-foreground">
        {hasUsableDefaults ? t("provider.readyTitle") : t("provider.title")}
      </h2>
      <p className="mt-2 max-w-[420px] text-sm leading-6 text-muted-foreground">
        {hasUsableDefaults
          ? t("provider.readyDescription")
          : t("provider.description")}
      </p>

      {hasUsableDefaults ? (
        <div className="mt-8 grid w-full gap-2 sm:grid-cols-2">
          {usableDefaultEntries.map(({ kind, entry }) => {
            const model = firstUsableModel(entry);
            const isSelecting = selectingProviderId === entry.providerId;
            return (
              <button
                key={`${kind}:${entry.providerId}`}
                type="button"
                onClick={() =>
                  kind === "model"
                    ? onSelectModelProvider(entry)
                    : onSelectAgentProvider(entry)
                }
                disabled={isSelecting}
                className="flex w-full items-center gap-3 rounded-[14px] border border-border px-3 py-3 text-left transition-colors hover:bg-muted/40 disabled:opacity-60"
              >
                <span className="flex size-9 shrink-0 items-center justify-center rounded-[10px] bg-muted text-foreground">
                  {getProviderIcon(entry.providerId, "size-4") ?? (
                    <IconPlugConnected className="size-4" strokeWidth={1.75} />
                  )}
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-medium text-foreground">
                    {entry.providerName}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">
                    {model?.name ?? entry.defaultModel}
                  </span>
                </span>
                {isSelecting ? (
                  <Spinner className="size-4 shrink-0 text-foreground" />
                ) : null}
              </button>
            );
          })}
        </div>
      ) : null}

      {!hasUsableDefaults && credentialLoading ? (
        <p className="mt-8 w-full rounded-[14px] border border-border px-3 py-3 text-sm text-muted-foreground">
          {t("provider.checking")}
        </p>
      ) : null}

      {providerError ? (
        <p
          role="alert"
          className="mt-4 w-full rounded-[10px] bg-danger/10 px-3 py-2 text-xs text-danger"
        >
          {providerError}
        </p>
      ) : null}

      {hasUsableDefaults && !showProviderSetup ? (
        <>
          <div className="mt-6 h-px w-full bg-border" />
          <button
            type="button"
            onClick={() => setShowProviderSetup(true)}
            className="mt-4 inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
          >
            <span aria-hidden="true" className="text-base leading-none">
              +
            </span>
            {t("provider.addDifferent")}
          </button>
        </>
      ) : null}

      {showProviderSetup ? (
        <div className="mt-6 w-full space-y-2 text-left">
          {hasUsableDefaults ? (
            <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
              {t("provider.setupTitle")}
            </p>
          ) : null}
          {modelProviders.map((provider) => (
            <ModelProviderRow
              key={provider.id}
              provider={{
                ...provider,
                status: configuredIds.has(provider.id)
                  ? "connected"
                  : "not_configured",
              }}
              onGetConfig={onGetConfig}
              onSaveFields={(fields) => onSave(provider.id, fields)}
              onRemoveConfig={() => onRemove(provider.id)}
              onCompleteNativeSetup={onCompleteNativeSetup}
              saving={savingProviderIds.has(provider.id)}
              inventorySyncing={syncingProviderIds.has(provider.id)}
              inventoryWarning={inventoryWarnings.get(provider.id)}
            />
          ))}
          {canBrowseAllProviders ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onBrowseAllProviders}
            >
              {t("provider.browseAll")}
            </Button>
          ) : null}
        </div>
      ) : null}

      {hasUsableDefaults ? (
        <div className="mt-6 flex items-center justify-center">
          <Button
            type="button"
            onClick={onContinue}
            rightIcon={<IconArrowRight />}
          >
            {t("provider.useCurrent")}
          </Button>
        </div>
      ) : null}
    </section>
  );
}
