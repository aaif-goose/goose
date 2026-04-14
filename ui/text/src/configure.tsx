import React, { useState, useEffect, useCallback } from "react";
import { Box, Text, useInput, useStdout } from "ink";
import type { GooseClient, ProviderDetailEntry } from "@aaif/goose-sdk";
import {
  CRANBERRY,
  TEAL,
  GOLD,
  TEXT_PRIMARY,
  TEXT_DIM,
  RULE_COLOR,
} from "./colors.js";
import { Spinner, SPINNER_FRAMES } from "./components/Spinner.js";
import { ErrorScreen } from "./components/ErrorScreen.js";
import { ProviderSelector, ProviderConfigurator } from "./onboarding.js";

const LOAD_MODELS_TIMEOUT_MS = 30000;

type Phase =
  | "loading"
  | "select_provider"
  | "configure"
  | "loading_models"
  | "select_model"
  | "saving"
  | "error";

export type ConfigureIntent = "provider" | "model";

interface ConfigureProps {
  client: GooseClient;
  sessionId: string;
  width: number;
  height: number;
  onComplete: () => void;
  onCancel: () => void;
  initialIntent?: ConfigureIntent;
}

interface ModelSelectorProps {
  client: GooseClient;
  provider: ProviderDetailEntry;
  height: number;
  onSelect: (model: string) => void;
  onBack: () => void;
}

const ModelSelector = React.memo(function ModelSelector({
  client,
  provider,
  height,
  onSelect,
  onBack,
}: ModelSelectorProps) {
  const [loading, setLoading] = useState(true);
  const [models, setModels] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [manualEntry, setManualEntry] = useState(false);
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;

  useEffect(() => {
    let cancelled = false;
    const timeoutId = setTimeout(() => {
      if (!cancelled) {
        setError("Request timed out. The provider may be slow to respond.");
        setLoading(false);
      }
    }, LOAD_MODELS_TIMEOUT_MS);

    (async () => {
      try {
        setLoading(true);
        setError(null);
        const resp = await client.goose.GooseProvidersModels({
          providerName: provider.name,
        });
        if (!cancelled) {
          setModels(resp.models);
          const defaultIdx = resp.models.findIndex((m) => m === provider.defaultModel);
          setSelectedIdx(defaultIdx >= 0 ? defaultIdx : 0);
          setLoading(false);
          clearTimeout(timeoutId);
        }
      } catch (e: unknown) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setLoading(false);
          clearTimeout(timeoutId);
        }
      }
    })();

    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
    };
  }, [client, provider.name, provider.defaultModel]);

  const filtered = (() => {
    if (!searchQuery) return models;
    const q = searchQuery.toLowerCase();
    return models.filter((m) => m.toLowerCase().includes(q));
  })();

  const maxWidth = Math.min(columns - 4, 80);
  const HEADER_HEIGHT = 2;
  const SEARCH_BOX_HEIGHT = 3;
  const FOOTER_HEIGHT = 3;
  const CHROME_HEIGHT = HEADER_HEIGHT + SEARCH_BOX_HEIGHT + FOOTER_HEIGHT + 4;
  const listHeight = Math.max(height - CHROME_HEIGHT, 3);
  const [scrollOffset, setScrollOffset] = useState(0);

  useEffect(() => {
    if (selectedIdx < scrollOffset) {
      setScrollOffset(selectedIdx);
    } else if (selectedIdx >= scrollOffset + listHeight) {
      setScrollOffset(selectedIdx - listHeight + 1);
    }
  }, [selectedIdx, scrollOffset, listHeight]);

  useInput((ch, key) => {
    if (key.escape) {
      if (manualEntry) {
        setManualEntry(false);
        setSearchQuery("");
        return;
      }
      if (searchQuery) {
        setSearchQuery("");
        setSelectedIdx(0);
        setScrollOffset(0);
        return;
      }
      onBack();
      return;
    }
    if (manualEntry) {
      if (key.return) {
        if (searchQuery.trim()) {
          onSelect(searchQuery.trim());
        }
        return;
      }
      if (key.backspace || key.delete) {
        setSearchQuery((q) => q.slice(0, -1));
        return;
      }
      if (ch && ch.length === 1 && !key.ctrl && !key.meta) {
        setSearchQuery((q) => q + ch);
      }
      return;
    }
    if (key.upArrow) {
      setSelectedIdx((i) => Math.max(i - 1, 0));
      return;
    }
    if (key.downArrow) {
      setSelectedIdx((i) => Math.min(i + 1, filtered.length - 1));
      return;
    }
    if (key.return) {
      const m = filtered[selectedIdx];
      if (m) onSelect(m);
      return;
    }
    if (key.backspace || key.delete) {
      setSearchQuery((q) => q.slice(0, -1));
      setSelectedIdx(0);
      setScrollOffset(0);
      return;
    }
    if (ch === "m" && !searchQuery) {
      setManualEntry(true);
      return;
    }
    if (ch && ch.length === 1 && !key.ctrl && !key.meta) {
      setSearchQuery((q) => q + ch);
      setSelectedIdx(0);
      setScrollOffset(0);
    }
  });

  if (loading) {
    return (
      <Box flexDirection="column" height={height} width={columns} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Select model ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>Loading models for {provider.displayName}…</Text>
        </Box>
        <Box justifyContent="center" flexGrow={1} alignItems="center">
          <Spinner idx={0} />
        </Box>
      </Box>
    );
  }

  if (error) {
    return (
      <Box flexDirection="column" height={height} width={columns} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Select model ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={GOLD}>⚠ Failed to load models</Text>
        </Box>
        <Box justifyContent="center">
          <Box width={maxWidth}>
            <Text color={TEXT_DIM} wrap="wrap">{error}</Text>
          </Box>
        </Box>
        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>m manual entry · esc back</Text>
        </Box>
      </Box>
    );
  }

  if (manualEntry) {
    const inputWidth = Math.min(60, maxWidth - 4);
    const displayText = searchQuery || "type model name…";
    const truncatedText = displayText.length > inputWidth - 6
      ? displayText.slice(0, inputWidth - 9) + "…"
      : displayText;

    return (
      <Box flexDirection="column" height={height} width={columns} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Enter model name ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>Type a model identifier for {provider.displayName}</Text>
        </Box>

        <Box justifyContent="center">
          <Box
            borderStyle="round"
            borderColor={GOLD}
            paddingX={2}
            width={inputWidth}
          >
            <Text color={GOLD} bold>{"❯ "}</Text>
            <Text color={searchQuery ? TEXT_PRIMARY : TEXT_DIM}>
              {truncatedText}
            </Text>
          </Box>
        </Box>

        <Box justifyContent="center" marginTop={2}>
          <Text color={TEXT_DIM}>enter confirm · esc cancel</Text>
        </Box>
      </Box>
    );
  }

  const visible = filtered.slice(scrollOffset, scrollOffset + listHeight);
  const searchBoxWidth = Math.min(60, maxWidth - 4);

  return (
    <Box flexDirection="column" height={height} width={columns} paddingX={2}>
      {/* Header */}
      <Box marginTop={1} />
      <Box justifyContent="center" marginBottom={1}>
        <Text color={TEXT_PRIMARY} bold>◆ Select model ◆</Text>
      </Box>
      <Box justifyContent="center" marginBottom={2}>
        <Text color={TEXT_DIM}>Choose a model for {provider.displayName}</Text>
      </Box>

      {/* Search Bar */}
      <Box justifyContent="center" marginBottom={2}>
        <Box
          borderStyle="round"
          borderColor={RULE_COLOR}
          paddingX={2}
          width={searchBoxWidth}
        >
          <Text color={CRANBERRY} bold>{"❯ "}</Text>
          <Box width={searchBoxWidth - 8}>
            <Text color={searchQuery ? TEXT_PRIMARY : TEXT_DIM} wrap="truncate">
              {searchQuery || "search models…"}
            </Text>
          </Box>
        </Box>
      </Box>

      {/* Model List */}
      <Box flexDirection="column" flexGrow={1} justifyContent="flex-start">
        {filtered.length === 0 ? (
          <Box justifyContent="center" alignItems="center" height={Math.max(listHeight, 1)}>
            <Text color={TEXT_DIM}>No matching models</Text>
          </Box>
        ) : (
          <>
            {scrollOffset > 0 && (
              <Box justifyContent="center" marginBottom={1}>
                <Text color={TEXT_DIM}>▲ {scrollOffset} more above</Text>
              </Box>
            )}
            <Box justifyContent="center">
              <Box flexDirection="column" width={maxWidth}>
                {visible.map((model, vi) => {
                  const idx = vi + scrollOffset;
                  const active = idx === selectedIdx;
                  const isDefault = model === provider.defaultModel;
                  const modelWidth = maxWidth - 8;
                  const truncatedModel = model.length > modelWidth
                    ? model.slice(0, modelWidth - 1) + "…"
                    : model;

                  return (
                    <Box key={model}>
                      <Text color={active ? GOLD : TEXT_DIM}>
                        {active ? "▸ " : "  "}
                      </Text>
                      <Text color={active ? TEXT_PRIMARY : TEXT_DIM} bold={active}>
                        {truncatedModel}
                      </Text>
                      {isDefault && <Text color={TEAL}> (default)</Text>}
                    </Box>
                  );
                })}
              </Box>
            </Box>
            {scrollOffset + listHeight < filtered.length && (
              <Box justifyContent="center" marginTop={1}>
                <Text color={TEXT_DIM}>
                  ▼ {filtered.length - scrollOffset - listHeight} more below
                </Text>
              </Box>
            )}
          </>
        )}
      </Box>

      {/* Footer */}
      <Box justifyContent="center" marginTop={2}>
        <Text color={TEXT_DIM}>
          ↑↓ navigate · enter select · m manual · esc back
        </Text>
      </Box>
    </Box>
  );
});

export default function ConfigureScreen({
  client,
  sessionId,
  width,
  height,
  onComplete,
  onCancel,
  initialIntent,
}: ConfigureProps) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [providers, setProviders] = useState<ProviderDetailEntry[]>([]);
  const [selectedProvider, setSelectedProvider] = useState<ProviderDetailEntry | null>(null);
  const [errorMsg, setErrorMsg] = useState("");
  const [spinIdx, setSpinIdx] = useState(0);
  const [fetchKey, setFetchKey] = useState(0);

  useEffect(() => {
    const t = setInterval(
      () => setSpinIdx((i) => (i + 1) % SPINNER_FRAMES.length),
      300,
    );
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const resp = await client.goose.GooseProvidersDetails({});
        if (cancelled) return;
        const sorted = [...resp.providers].sort((a, b) => {
          const aP = a.providerType === "Preferred" ? 0 : 1;
          const bP = b.providerType === "Preferred" ? 0 : 1;
          if (aP !== bP) return aP - bP;
          return a.displayName.localeCompare(b.displayName);
        });
        setProviders(sorted);

        if (initialIntent === "model") {
          try {
            const cfg = await client.goose.GooseConfigRead({ key: "GOOSE_PROVIDER" });
            if (cancelled) return;
            const current = sorted.find((p) => p.name === cfg.value);
            if (current) {
              setSelectedProvider(current);
              setPendingConfigValues({});
              setPhase("select_model");
              return;
            }
          } catch {
            // fall through to provider selector
          }
        }

        if (!cancelled) setPhase("select_provider");
      } catch (e: unknown) {
        if (!cancelled) {
          setErrorMsg(e instanceof Error ? e.message : String(e));
          setPhase("error");
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, fetchKey, initialIntent]);

  const applyProviderModel = useCallback(
    async (provider: ProviderDetailEntry, model: string, configValues: Record<string, string>) => {
      setPhase("saving");
      try {
        for (const [key, value] of Object.entries(configValues)) {
          const configKey = provider.configKeys.find((k) => k.name === key);
          if (configKey?.secret) {
            await client.goose.GooseSecretUpsert({ key, value });
          } else {
            await client.goose.GooseConfigUpsert({ key, value });
          }
        }
        await client.goose.GooseConfigUpsert({ key: "GOOSE_PROVIDER", value: provider.name });
        await client.goose.GooseConfigUpsert({ key: "GOOSE_MODEL", value: model });
        await client.goose.GooseSessionProviderUpdate({
          sessionId,
          provider: provider.name,
          model,
        });
        onComplete();
      } catch (e: unknown) {
        setErrorMsg(e instanceof Error ? e.message : String(e));
        setPhase("error");
      }
    },
    [client, sessionId, onComplete],
  );

  const [pendingConfigValues, setPendingConfigValues] = useState<Record<string, string>>({});

  const handleProviderSelected = useCallback(
    (provider: ProviderDetailEntry) => {
      const keys = provider.configKeys.filter(
        (k) => k.required && !k.oauthFlow && !k.deviceCodeFlow,
      );
      setSelectedProvider(provider);
      if (keys.length > 0 && !provider.isConfigured) {
        setPhase("configure");
      } else {
        setPendingConfigValues({});
        setPhase("select_model");
      }
    },
    [],
  );

  const handleConfigComplete = useCallback(
    (values: Record<string, string>) => {
      if (!selectedProvider) return;
      setPendingConfigValues(values);
      setPhase("select_model");
    },
    [selectedProvider],
  );

  const handleModelSelected = useCallback(
    (model: string) => {
      if (!selectedProvider) return;
      applyProviderModel(selectedProvider, model, pendingConfigValues);
    },
    [selectedProvider, pendingConfigValues, applyProviderModel],
  );

  const handleRetry = useCallback(() => {
    setErrorMsg("");
    setFetchKey((k) => k + 1);
    setPhase("loading");
  }, []);

  if (phase === "loading" || phase === "loading_models" || phase === "saving") {
    const label = 
      phase === "loading" ? "Loading providers…" : 
      phase === "loading_models" ? "Loading models…" :
      "Applying changes…";
    return (
      <Box flexDirection="column" height={height} width={width} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Configure provider ◆</Text>
        </Box>
        <Box justifyContent="center" marginBottom={2}>
          <Text color={TEXT_DIM}>{label}</Text>
        </Box>
        <Box justifyContent="center" flexGrow={1} alignItems="center">
          <Spinner idx={spinIdx} />
        </Box>
      </Box>
    );
  }

  if (phase === "error") {
    return (
      <Box flexDirection="column" height={height} width={width} paddingX={2}>
        <Box marginTop={1} />
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>◆ Configure provider ◆</Text>
        </Box>
        <ErrorScreen errorMsg={errorMsg} onRetry={handleRetry} />
      </Box>
    );
  }

  if (phase === "configure" && selectedProvider) {
    return (
      <ProviderConfigurator
        provider={selectedProvider}
        height={height}
        onComplete={handleConfigComplete}
        onBack={() => {
          setSelectedProvider(null);
          setPhase("select_provider");
        }}
      />
    );
  }

  if (phase === "select_model" && selectedProvider) {
    return (
      <ModelSelector
        client={client}
        provider={selectedProvider}
        height={height}
        onSelect={handleModelSelected}
        onBack={() => {
          if (initialIntent === "model") {
            onCancel();
          } else {
            setPhase("select_provider");
          }
        }}
      />
    );
  }

  return (
    <ProviderSelector
      providers={providers}
      height={height}
      onSelect={handleProviderSelected}
      title="◆ Configure provider ◆"
      subtitle="Select a provider and model for this session"
      onBack={onCancel}
    />
  );
}
