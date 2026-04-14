import React, { useState, useEffect, useCallback } from "react";
import { Box, Text, useInput, useStdout } from "ink";
import { TextInput, PasswordInput } from '@inkjs/ui';
import type { GooseClient, ProviderDetailEntry } from "@aaif/goose-sdk";
import {
  CRANBERRY,
  TEAL,
  GOLD,
  TEXT_PRIMARY,
  TEXT_SECONDARY,
  TEXT_DIM,
  RULE_COLOR,
} from "./colors.js";
import { Spinner, SPINNER_FRAMES } from "./components/Spinner.js";
import { ErrorScreen } from "./components/ErrorScreen.js";

type Phase =
  | "loading"
  | "select_provider"
  | "configure"
  | "saving"
  | "success"
  | "error";

interface OnboardingProps {
  client: GooseClient;
  width: number;
  height: number;
  onComplete: () => void;
}

export interface ProviderSelectorProps {
  providers: ProviderDetailEntry[];
  height: number;
  onSelect: (provider: ProviderDetailEntry) => void;
  title?: string;
  subtitle?: string;
  onBack?: () => void;
}

export const ProviderSelector = React.memo(function ProviderSelector({ providers, height, onSelect, title, subtitle, onBack }: ProviderSelectorProps) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;

  const filtered = (() => {
    if (!searchQuery) return providers;
    const q = searchQuery.toLowerCase();
    return providers.filter(
      (p) =>
        p.displayName.toLowerCase().includes(q) ||
        p.name.toLowerCase().includes(q),
    );
  })();

  // Calculate grid dimensions based on terminal size
  const cardWidth = 36;  // Width of each provider card
  const cardHeight = 8;  // Height of each provider card
  const minSpacing = 2;  // Minimum spacing between cards
  
  const availableWidth = columns - 4; // Leave margins
  // Header: marginTop(1) + title+mb(2) + subtitle+mb(3) + searchbar+mb(5) = 11
  // Footer: mt(2) + text(1) = 3, plus potential scroll indicators(2)
  const availableHeight = height - 16;
  
  const cardsPerRow = Math.max(1, Math.floor(availableWidth / (cardWidth + minSpacing)));
  // Cap horizontal gap so it doesn't grow unbounded on wide terminals
  const columnSpacing = Math.min(minSpacing, Math.floor((availableWidth - (cardsPerRow * cardWidth)) / Math.max(1, cardsPerRow - 1)));
  // Terminal chars are ~2× taller than wide, so 1 row ≈ 2 columns visually
  const rowSpacing = 1;
  const rowsVisible = Math.max(1, Math.floor((availableHeight + rowSpacing) / (cardHeight + rowSpacing)));
  
  const totalRows = Math.ceil(filtered.length / cardsPerRow);
  const selectedRow = Math.floor(selectedIdx / cardsPerRow);
  // Calculate scroll offset for rows
  const [scrollRow, setScrollRow] = useState(0);
  
  useEffect(() => {
    if (selectedRow < scrollRow) {
      setScrollRow(selectedRow);
    } else if (selectedRow >= scrollRow + rowsVisible) {
      setScrollRow(selectedRow - rowsVisible + 1);
    }
  }, [selectedRow, rowsVisible, scrollRow]);

  useInput((ch, key) => {
    if (key.escape) {
      if (searchQuery) {
        setSearchQuery("");
        setSelectedIdx(0);
        setScrollRow(0);
        return;
      }
      if (onBack) {
        onBack();
        return;
      }
    }
    if (filtered.length === 0) {
      // Only allow typing/backspace when no results match; skip navigation
      if (key.backspace || key.delete) {
        setSearchQuery((q) => q.slice(0, -1));
        setSelectedIdx(0);
        setScrollRow(0);
        return;
      }
      if (ch && ch.length === 1 && !key.ctrl && !key.meta) {
        setSearchQuery((q) => q + ch);
        setSelectedIdx(0);
        setScrollRow(0);
      }
      return;
    }
    if (key.upArrow) {
      const newIdx = Math.max(selectedIdx - cardsPerRow, 0);
      setSelectedIdx(newIdx);
      return;
    }
    if (key.downArrow) {
      const newIdx = Math.min(selectedIdx + cardsPerRow, filtered.length - 1);
      setSelectedIdx(newIdx);
      return;
    }
    if (key.leftArrow) {
      const newIdx = Math.max(selectedIdx - 1, 0);
      setSelectedIdx(newIdx);
      return;
    }
    if (key.rightArrow) {
      const newIdx = Math.min(selectedIdx + 1, filtered.length - 1);
      setSelectedIdx(newIdx);
      return;
    }
    if (key.return) {
      const p = filtered[selectedIdx];
      if (p) onSelect(p);
      return;
    }
    if (key.backspace || key.delete) {
      setSearchQuery((q) => q.slice(0, -1));
      setSelectedIdx(0);
      setScrollRow(0);
      return;
    }
    if (ch && ch.length === 1 && !key.ctrl && !key.meta) {
      setSearchQuery((q) => q + ch);
      setSelectedIdx(0);
      setScrollRow(0);
    }
  });

  // Create grid of provider cards
  const renderProviderCard = (provider: ProviderDetailEntry, _index: number, isSelected: boolean) => {
    const cardBorder = isSelected ? "double" : "single";
    const cardBorderColor = isSelected ? GOLD : RULE_COLOR;
    const textColor = isSelected ? TEXT_PRIMARY : TEXT_SECONDARY;
    
    // Calculate actual content width: cardWidth - borders (2) - paddingX (2)
    const contentWidth = cardWidth - 4;
    // Width for title (leave space for icons: 2-3 chars)
    const titleWidth = contentWidth - 3;
    // Available lines for description: cardHeight - borders (2) - title (1) - margin (1) - name (1) - margin (1)
    const descriptionMaxLines = Math.max(1, cardHeight - 6);
    const descriptionMaxChars = descriptionMaxLines * contentWidth;
    
    return (
      <Box
        key={provider.name}
        width={cardWidth}
        height={cardHeight}
        borderStyle={cardBorder}
        borderColor={cardBorderColor}
        paddingX={1}
        paddingY={0}
        flexDirection="column"
      >
        <Box justifyContent="space-between" alignItems="center">
          <Box width={titleWidth} flexShrink={1}>
            <Text color={textColor} bold={isSelected} wrap="truncate">
              {provider.displayName}
            </Text>
          </Box>
          <Box flexShrink={0}>
            {provider.providerType === "Preferred" && (
              <Text color={TEAL}>★</Text>
            )}
            {provider.isConfigured && (
              <Text color={TEAL}>✓</Text>
            )}
          </Box>
        </Box>
        
        <Box marginTop={1} flexDirection="column" flexGrow={1}>
          <Box width={contentWidth}>
            <Text color={TEXT_DIM} wrap="truncate">
              {provider.name}
            </Text>
          </Box>
          {provider.description && (
            <Box marginTop={1} width={contentWidth}>
              <Text color={TEXT_DIM} wrap="truncate" dimColor>
                {provider.description.length > descriptionMaxChars
                  ? provider.description.slice(0, descriptionMaxChars - 1) + "…"
                  : provider.description}
              </Text>
            </Box>
          )}
        </Box>
      </Box>
    );
  };

  const visibleRows = [];
  for (let row = scrollRow; row < Math.min(scrollRow + rowsVisible, totalRows); row++) {
    const rowProviders = [];
    for (let col = 0; col < cardsPerRow; col++) {
      const index = row * cardsPerRow + col;
      if (index < filtered.length) {
        const isSelected = index === selectedIdx;
        rowProviders.push(renderProviderCard(filtered[index], index, isSelected));
      }
    }
    
    if (rowProviders.length > 0) {
      const isLastVisibleRow = row === Math.min(scrollRow + rowsVisible, totalRows) - 1;
      visibleRows.push(
        <Box key={row} gap={columnSpacing} marginBottom={isLastVisibleRow ? 0 : rowSpacing}>
          {rowProviders}
        </Box>
      );
    }
  }

  return (
    <Box flexDirection="column" height={height} width={columns} paddingX={2}>
      {/* Header */}
      <Box marginTop={1} />
      <Box justifyContent="center" marginBottom={1}>
        <Text color={TEXT_PRIMARY} bold>
          {title ?? "◆ Welcome to goose ◆"}
        </Text>
      </Box>
      <Box justifyContent="center" marginBottom={2}>
        <Text color={TEXT_DIM}>
          {subtitle ?? "Connect an AI model provider to get started"}
        </Text>
      </Box>
      
      {/* Search Bar */}
      <Box justifyContent="center" marginBottom={2}>
        <Box
          borderStyle="round"
          borderColor={RULE_COLOR}
          paddingX={2}
          width={Math.min(60, availableWidth)}
        >
          <Text color={CRANBERRY} bold>
            {"❯ "}
          </Text>
          <Text color={searchQuery ? TEXT_PRIMARY : TEXT_DIM} wrap="truncate">
            {searchQuery || "search providers…"}
          </Text>
        </Box>
      </Box>

      {/* Provider Grid */}
      <Box flexDirection="column" flexGrow={1} justifyContent="flex-start">
        {filtered.length === 0 ? (
          <Box justifyContent="center" alignItems="center" height={10}>
            <Text color={TEXT_DIM}>No matching providers found</Text>
          </Box>
        ) : (
          <>
            {scrollRow > 0 && (
              <Box justifyContent="center" marginBottom={1}>
                <Text color={TEXT_DIM}>▲ {scrollRow * cardsPerRow} more above</Text>
              </Box>
            )}
            
            <Box justifyContent="center">
              <Box flexDirection="column">
                {visibleRows}
              </Box>
            </Box>
            
            {scrollRow + rowsVisible < totalRows && (
              <Box justifyContent="center" marginTop={1}>
                <Text color={TEXT_DIM}>
                  ▼ {filtered.length - (scrollRow + rowsVisible) * cardsPerRow} more below
                </Text>
              </Box>
            )}
          </>
        )}
      </Box>

      {/* Footer */}
      <Box justifyContent="center" marginTop={2}>
        <Text color={TEXT_DIM}>
          ↑↓←→ navigate · enter select · type to search{onBack ? " · esc back" : " · esc clear"}
        </Text>
      </Box>
    </Box>
  );
});

export interface ProviderConfiguratorProps {
  provider: ProviderDetailEntry;
  height: number;
  onComplete: (values: Record<string, string>) => void;
  onBack: () => void;
}

export const ProviderConfigurator = React.memo(function ProviderConfigurator({ provider, height, onComplete, onBack }: ProviderConfiguratorProps) {
  const [keyValues, setKeyValues] = useState<Record<string, string>>({});
  const [activeKeyIdx, setActiveKeyIdx] = useState(0);
  const [showMasked, setShowMasked] = useState<Record<string, boolean>>({});
  const [inputKey, setInputKey] = useState(0);
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;

  const keys = provider.configKeys.filter(
    (k) => k.required && !k.oauthFlow && !k.deviceCodeFlow,
  );
  const currentKey = keys[activeKeyIdx];

  useInput((_ch, key) => {
    if (!currentKey) return;

    if (key.escape) {
      onBack();
      return;
    }
    if (key.tab && currentKey.secret) {
      setShowMasked((prev) => ({
        ...prev,
        [currentKey.name]: !prev[currentKey.name],
      }));
      return;
    }
  });

  const handleSubmit = (value: string) => {
    if (!currentKey) return;
    const effective = value.trim() || currentVal.trim();
    if (!effective) return;
    const newValues = { ...keyValues, [currentKey.name]: effective };
    setKeyValues(newValues);
    if (activeKeyIdx < keys.length - 1) {
      setActiveKeyIdx(activeKeyIdx + 1);
      setShowMasked({});
      setInputKey(prev => prev + 1); // Force new input component
    } else {
      onComplete(newValues);
    }
  };

  const handleChange = (value: string) => {
    if (!currentKey) return;
    setKeyValues((prev) => ({
      ...prev,
      [currentKey.name]: value,
    }));
  };

  const currentVal = keyValues[currentKey?.name ?? ""] ?? "";
  const masked = currentKey?.secret && !showMasked[currentKey?.name ?? ""];
  const maxWidth = Math.min(columns - 4, 80);

  // Calculate content height for proper centering
  const headerHeight = 1 + (provider.description ? 2 : 0) + 1; // title + description + spacer
  const keysHeight = keys.length; // one line per key
  const inputHeight = currentKey ? 3 : 0; // input + help text + spacing
  const setupStepsHeight = provider.setupSteps?.length ? provider.setupSteps.length + 1 : 0;
  const contentHeight = headerHeight + keysHeight + inputHeight + setupStepsHeight;
  const topPad = Math.max(0, Math.floor((height - contentHeight) / 2));

  return (
    <Box flexDirection="column" height={height} alignItems="center" width={columns}>
      {topPad > 0 && <Box height={topPad} />}
      <Box flexDirection="column" width={maxWidth} paddingX={2}>
        {/* Header */}
        <Box justifyContent="center" marginBottom={1}>
          <Text color={TEXT_PRIMARY} bold>
            ◆ Configure {provider.displayName} ◆
          </Text>
        </Box>
        {provider.description && (
          <Box justifyContent="center" marginBottom={1}>
            <Box width={maxWidth - 4}>
              <Text color={TEXT_DIM} wrap="wrap">{provider.description}</Text>
            </Box>
          </Box>
        )}
        <Box marginTop={1} />

        {/* Configuration Keys */}
        {keys.map((k, i) => (
          <Box key={k.name} marginBottom={1}>
            <Text color={i === activeKeyIdx ? GOLD : TEXT_DIM}>
              {i < activeKeyIdx ? "✓ " : i === activeKeyIdx ? "▸ " : "  "}
            </Text>
            <Text
              color={i === activeKeyIdx ? TEXT_PRIMARY : TEXT_DIM}
              bold={i === activeKeyIdx}
            >
              {k.name}
            </Text>
            {i < activeKeyIdx && (
              <Text color={TEAL}> ••••••</Text>
            )}
          </Box>
        ))}

        {/* Current Input Field */}
        {currentKey && (
          <Box marginTop={1} flexDirection="column">
            <Box>
              <Text color={CRANBERRY} bold>
                {"❯ "}
              </Text>
              {masked ? (
                <PasswordInput
                  key={`password-${currentKey.name}-${inputKey}`}
                  placeholder={currentKey.name}
                  onChange={handleChange}
                  onSubmit={handleSubmit}
                />
              ) : (
                <TextInput
                  key={`text-${currentKey.name}-${inputKey}`}
                  defaultValue={currentVal}
                  placeholder={currentKey.name}
                  onChange={handleChange}
                  onSubmit={handleSubmit}
                />
              )}
            </Box>
            <Box marginTop={1}>
              <Box width={maxWidth - 4}>
                <Text color={TEXT_DIM} wrap="wrap">
                  enter confirm · esc back
                  {currentKey.secret && (
                    <>
                      {" · tab "}
                      {masked ? "reveal" : "hide"}
                    </>
                  )}
                </Text>
              </Box>
            </Box>
          </Box>
        )}

        {/* Setup Steps */}
        {provider.setupSteps &&
          provider.setupSteps.length > 0 && (
            <Box marginTop={2} flexDirection="column">
              <Text color={TEXT_DIM}>Setup steps:</Text>
              {provider.setupSteps.map((step, i) => (
                <Box key={i} width={maxWidth - 4} marginTop={1}>
                  <Text color={TEXT_DIM} wrap="wrap">
                    {i + 1}. {step}
                  </Text>
                </Box>
              ))}
            </Box>
          )}
      </Box>
    </Box>
  );
});

interface SuccessScreenProps {
  provider: ProviderDetailEntry | null;
  height: number;
}

const SuccessScreen = React.memo(function SuccessScreen({ provider, height }: SuccessScreenProps) {
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;

  // Calculate content height for proper centering
  const contentHeight = 1 + (provider ? 1 : 0); // success message + provider text
  const topPad = Math.max(0, Math.floor((height - contentHeight) / 2));

  return (
    <Box
      flexDirection="column"
      alignItems="center"
      width={columns}
      height={height}
      overflow="hidden"
    >
      {topPad > 0 && <Box height={topPad} />}
      <Box flexDirection="column" alignItems="center">
        <Text color={TEAL} bold>
          ✓ Provider configured
        </Text>
        {provider && (
          <Box marginTop={1}>
            <Text color={TEXT_SECONDARY}>
              Connected to {provider.displayName}
            </Text>
          </Box>
        )}
      </Box>
    </Box>
  );
});

export default function Onboarding({
  client,
  width,
  height,
  onComplete,
}: OnboardingProps) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [providers, setProviders] = useState<ProviderDetailEntry[]>([]);
  const [selectedProvider, setSelectedProvider] =
    useState<ProviderDetailEntry | null>(null);
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
    (async () => {
      try {
        const resp = await client.goose.GooseProvidersDetails({});
        const sorted = [...resp.providers].sort((a, b) => {
          const aP = a.providerType === "Preferred" ? 0 : 1;
          const bP = b.providerType === "Preferred" ? 0 : 1;
          if (aP !== bP) return aP - bP;
          return a.displayName.localeCompare(b.displayName);
        });
        setProviders(sorted);
        setPhase("select_provider");
      } catch (e: unknown) {
        setErrorMsg(e instanceof Error ? e.message : JSON.stringify(e));
        setPhase("error");
      }
    })();
  }, [client, fetchKey]);

  const saveProvider = useCallback(
    async (provider: ProviderDetailEntry, values: Record<string, string>) => {
      setPhase("saving");
      try {
        for (const [key, value] of Object.entries(values)) {
          const configKey = provider.configKeys.find((k) => k.name === key);
          if (configKey?.secret) {
            await client.goose.GooseSecretUpsert({ key, value });
          } else {
            await client.goose.GooseConfigUpsert({ key, value });
          }
        }
        await client.goose.GooseConfigUpsert({
          key: "GOOSE_PROVIDER",
          value: provider.name,
        });
        await client.goose.GooseConfigUpsert({
          key: "GOOSE_MODEL",
          value: provider.defaultModel,
        });
        setPhase("success");
        setTimeout(onComplete, 1000);
      } catch (e: unknown) {
        setErrorMsg(e instanceof Error ? e.message : JSON.stringify(e));
        setPhase("error");
      }
    },
    [client, onComplete],
  );

  const confirmProvider = useCallback(
    (provider: ProviderDetailEntry) => {
      const keys = provider.configKeys.filter(
        (k) => k.required && !k.oauthFlow && !k.deviceCodeFlow,
      );
      if (keys.length === 0) {
        saveProvider(provider, {});
        return;
      }
      setSelectedProvider(provider);
      setPhase("configure");
    },
    [saveProvider],
  );

  const handleRetry = useCallback(() => {
    setErrorMsg("");
    setFetchKey((k) => k + 1);
    setPhase("loading");
  }, []);

  if (phase === "loading") {
    const contentHeight = 3; // spinner + text + spacing
    const topPad = Math.max(0, Math.floor((height - contentHeight) / 2));
    
    return (
      <Box
        flexDirection="column"
        alignItems="center"
        width={width}
        height={height}
        overflow="hidden"
      >
        {topPad > 0 && <Box height={topPad} />}
        <Box flexDirection="column" alignItems="center">
          <Spinner idx={spinIdx} />
          <Box marginTop={1}>
            <Text color={TEXT_DIM}>loading providers…</Text>
          </Box>
        </Box>
      </Box>
    );
  }

  if (phase === "error") {
    return (
      <Box flexDirection="column" height={height} alignItems="center" width={width}>
        <ErrorScreen errorMsg={errorMsg} onRetry={handleRetry} />
      </Box>
    );
  }

  if (phase === "saving") {
    const contentHeight = 3; // spinner + text + spacing
    const topPad = Math.max(0, Math.floor((height - contentHeight) / 2));
    
    return (
      <Box
        flexDirection="column"
        alignItems="center"
        width={width}
        height={height}
        overflow="hidden"
      >
        {topPad > 0 && <Box height={topPad} />}
        <Box flexDirection="column" alignItems="center">
          <Spinner idx={spinIdx} />
          <Box marginTop={1}>
            <Text color={TEXT_DIM}>saving configuration…</Text>
          </Box>
        </Box>
      </Box>
    );
  }

  if (phase === "success") {
    return (
      <SuccessScreen provider={selectedProvider} height={height} />
    );
  }

  if (phase === "configure" && selectedProvider) {
    return (
      <ProviderConfigurator
        provider={selectedProvider}
        height={height}
        onComplete={(values) => saveProvider(selectedProvider, values)}
        onBack={() => {
          setSelectedProvider(null);
          setPhase("select_provider");
        }}
      />
    );
  }

  return (
    <ProviderSelector
      providers={providers}
      height={height}
      onSelect={confirmProvider}
    />
  );
}
