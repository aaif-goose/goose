import { useState, useEffect, useRef, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";
import { getProviderIcon } from "@/shared/ui/icons/ProviderIcons";
import { IconCheck, IconAlertTriangle, IconPlus } from "@tabler/icons-react";
import {
  checkAgentInstalled,
  checkAgentAuth,
  installAgent,
  authenticateAgent,
  onAgentSetupOutput,
} from "@/features/providers/api/agentSetup";
import type { ProviderDisplayInfo } from "@/shared/types/providers";

type SetupPhase = "idle" | "checking" | "installing" | "authenticating";

interface OutputLine {
  id: number;
  text: string;
}

const MAX_OUTPUT_LINES = 50;

interface AgentProviderCardProps {
  provider: ProviderDisplayInfo;
}

export function AgentProviderCard({ provider }: AgentProviderCardProps) {
  const { t } = useTranslation(["settings", "common"]);
  const [setupPhase, setSetupPhase] = useState<SetupPhase>("idle");
  const [setupOutput, setSetupOutput] = useState<OutputLine[]>([]);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [isInstalled, setIsInstalled] = useState<boolean | null>(null);
  const [isAuthenticated, setIsAuthenticated] = useState<boolean | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);
  const outputLengthRef = useRef(0);
  const lineCounterRef = useRef(0);
  const isMountedRef = useRef(true);
  const unlistenRef = useRef<(() => void) | null>(null);

  const icon = getProviderIcon(provider.id, "size-6");
  const isBuiltIn = provider.status === "built_in";
  const isActive = setupPhase !== "idle";
  const hasInstallCommand = !!provider.installCommand;
  const hasAuthCommand = !!provider.authCommand;
  const hasAuthStatusCheck = !!provider.authStatusCommand;
  const hasBinary = !!provider.binaryName;
  const authStorageKey = `agent-provider-auth:${provider.id}`;

  const setAuthHint = useCallback(
    (value: boolean) => {
      if (value) {
        localStorage.setItem(authStorageKey, "true");
      } else {
        localStorage.removeItem(authStorageKey);
      }
    },
    [authStorageKey],
  );

  const getAuthHint = useCallback(() => {
    return localStorage.getItem(authStorageKey) === "true";
  }, [authStorageKey]);

  const clearListener = useCallback(() => {
    unlistenRef.current?.();
    unlistenRef.current = null;
  }, []);

  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
      clearListener();
    };
  }, [clearListener]);

  useEffect(() => {
    if (!hasBinary || isBuiltIn || !provider.binaryName) return;

    checkAgentInstalled(provider.id)
      .then((installed) => {
        if (!isMountedRef.current) return;
        setIsInstalled(installed);
        if (installed && provider.authStatusCommand) {
          return checkAgentAuth(provider.id).then((authenticated) => {
            if (!isMountedRef.current) return;
            setIsAuthenticated(authenticated);
          });
        }
        if (installed && !provider.authStatusCommand) {
          setIsAuthenticated(getAuthHint() ? true : null);
        }
        if (!installed) {
          setIsAuthenticated(null);
          setAuthHint(false);
        }
      })
      .catch(() => {
        if (!isMountedRef.current) return;
        setIsInstalled(false);
      });
  }, [
    getAuthHint,
    hasBinary,
    isBuiltIn,
    provider.id,
    provider.binaryName,
    provider.authStatusCommand,
    setAuthHint,
  ]);

  useEffect(() => {
    if (outputRef.current && outputLengthRef.current !== setupOutput.length) {
      outputLengthRef.current = setupOutput.length;
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  });

  const appendOutput = useCallback((line: string) => {
    lineCounterRef.current += 1;
    const entry: OutputLine = { id: lineCounterRef.current, text: line };
    setSetupOutput((prev) => {
      const next = [...prev, entry];
      return next.length > MAX_OUTPUT_LINES
        ? next.slice(-MAX_OUTPUT_LINES)
        : next;
    });
  }, []);

  async function handleConnect() {
    setSetupError(null);
    setSetupOutput([]);
    lineCounterRef.current = 0;

    if (hasInstallCommand && isInstalled === false) {
      await runInstall();
    } else if (hasAuthCommand) {
      await runAuth();
    }
  }

  async function runInstall() {
    if (!provider.installCommand) return;
    setSetupPhase("installing");

    clearListener();
    const unlisten = await onAgentSetupOutput(provider.id, appendOutput);
    if (!isMountedRef.current) {
      unlisten();
      return;
    }
    unlistenRef.current = unlisten;

    try {
      await installAgent(provider.id);
      clearListener();
      if (!isMountedRef.current) return;

      if (hasBinary && provider.binaryName) {
        setSetupPhase("checking");
        const installed = await checkAgentInstalled(provider.binaryName);
        if (!isMountedRef.current) return;
        setIsInstalled(installed);
        if (!installed) {
          setAuthHint(false);
          setSetupError(
            "Install finished but the CLI was not found on PATH. You may need to restart your terminal.",
          );
          setSetupPhase("idle");
          return;
        }
      }

      if (hasAuthCommand) {
        await runAuth();
      } else {
        if (!isMountedRef.current) return;
        setSetupPhase("idle");
      }
    } catch (err) {
      clearListener();
      if (!isMountedRef.current) return;
      setSetupError(err instanceof Error ? err.message : String(err));
      setSetupPhase("idle");
    }
  }

  async function runAuth() {
    if (!provider.authCommand) return;
    setSetupPhase("authenticating");
    setSetupOutput([]);

    clearListener();
    const unlisten = await onAgentSetupOutput(provider.id, appendOutput);
    if (!isMountedRef.current) {
      unlisten();
      return;
    }
    unlistenRef.current = unlisten;

    try {
      await authenticateAgent(provider.id);
      clearListener();
      if (!isMountedRef.current) return;
      setAuthHint(true);
      setIsAuthenticated(true);
      setSetupPhase("idle");
    } catch (err) {
      clearListener();
      if (!isMountedRef.current) return;
      setSetupError(err instanceof Error ? err.message : String(err));
      setSetupPhase("idle");
    }
  }

  function handleRetry() {
    setSetupError(null);
    void handleConnect();
  }

  if (provider.showOnlyWhenInstalled && isInstalled !== true) return null;

  const isReady =
    isBuiltIn ||
    (isInstalled === true && !hasAuthCommand) ||
    (isInstalled === true && isAuthenticated === true);
  const needsAuth =
    isInstalled === true && hasAuthCommand && isAuthenticated !== true;
  const needsInstall = isInstalled === false && hasInstallCommand;

  function renderStatusIndicator() {
    if (isBuiltIn || isReady) {
      return (
        <div className="flex h-6 flex-shrink-0 items-center">
          <IconCheck className="size-4 text-success" />
        </div>
      );
    }

    if (setupError) {
      return (
        <div className="flex h-6 flex-shrink-0 items-center">
          <IconAlertTriangle className="size-4 text-danger" />
        </div>
      );
    }

    if (needsAuth && !isActive) {
      return (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => void handleConnect()}
          className="flex-shrink-0 text-muted-foreground"
          aria-label={t("providers.agents.signInLabel", {
            name: provider.displayName,
          })}
        >
          {t("providers.agents.signIn")}
        </Button>
      );
    }

    if (needsInstall && !isActive) {
      return (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={() => void handleConnect()}
          disabled={isInstalled === null && hasBinary}
          className="flex-shrink-0 text-muted-foreground"
          aria-label={t("providers.agents.installLabel", {
            name: provider.displayName,
          })}
        >
          <IconPlus className="size-4" />
        </Button>
      );
    }

    return null;
  }

  function renderStatusText() {
    if (isBuiltIn || isReady) return null;
    if (setupError) return "Setup failed";

    if (isInstalled === null && hasBinary) return "Checking...";
    if (isInstalled === true && hasAuthStatusCheck && isAuthenticated === null)
      return "Checking...";

    return null;
  }

  function renderAction() {
    if (isBuiltIn || isReady) return null;

    if (isActive) return null;

    if (setupError) {
      return (
        <Button type="button" variant="outline" size="xs" onClick={handleRetry}>
          {t("common:actions.retry")}
        </Button>
      );
    }

    return null;
  }

  function renderSetupProgress() {
    if (!isActive) return null;

    const phaseLabel =
      setupPhase === "installing"
        ? `Installing ${provider.displayName}...`
        : setupPhase === "authenticating"
          ? "Waiting for sign-in..."
          : "Verifying installation...";

    const stepInfo =
      setupPhase === "installing" && hasAuthCommand
        ? "Step 1 of 2"
        : setupPhase === "authenticating" && hasInstallCommand
          ? "Step 2 of 2"
          : null;

    return (
      <div className="mt-3 space-y-2 border-t pt-3">
        <div className="flex items-center gap-2">
          <Spinner className="size-3.5 text-accent" />
          <div className="min-w-0 flex-1">
            <span className="text-xs font-medium">{phaseLabel}</span>
            {stepInfo && (
              <span className="ml-2 text-xxs text-muted-foreground">
                {stepInfo}
              </span>
            )}
          </div>
        </div>

        {setupOutput.length > 0 && (
          <div
            ref={outputRef}
            className="max-h-24 overflow-y-auto rounded-md bg-muted px-2 py-1.5 font-mono text-xxs leading-relaxed text-muted-foreground"
          >
            {setupOutput.map((entry) => (
              <div key={entry.id}>{entry.text || "\u00A0"}</div>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex flex-col rounded-lg border bg-background p-3 transition-colors",
        isActive && "border-accent/50",
      )}
    >
      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex size-6 items-center justify-center [&>*]:size-6">
            {icon}
          </div>
          <span className="mt-2 block text-sm">{provider.displayName}</span>
          <p className="mt-1 text-xs text-muted-foreground">
            {provider.description}
          </p>
        </div>
        {renderStatusIndicator()}
      </div>

      {(() => {
        const statusText = renderStatusText();
        const action = renderAction();
        if (!statusText && !action) return null;
        return (
          <div className="mt-3 flex items-center justify-between">
            <div className="flex items-center gap-1.5">
              {statusText && (
                <>
                  <span
                    className={cn(
                      "size-1.5 rounded-full",
                      setupError
                        ? "bg-danger"
                        : isActive
                          ? "bg-accent animate-pulse"
                          : "bg-muted-foreground/40",
                    )}
                  />
                  <span className="text-xs text-muted-foreground">
                    {statusText}
                  </span>
                </>
              )}
            </div>
            {action}
          </div>
        );
      })()}

      {renderSetupProgress()}

      {setupError && !isActive && (
        <div className="mt-3 space-y-2 border-t pt-3">
          <p className="text-xs text-danger">{setupError}</p>
          {setupOutput.length > 0 && (
            <div className="max-h-24 overflow-y-auto rounded-md bg-muted px-2 py-1.5 font-mono text-xxs leading-relaxed text-muted-foreground">
              {setupOutput.map((entry) => (
                <div key={entry.id}>{entry.text || "\u00A0"}</div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
