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
type InstallStatus = "checking" | "installed" | "missing";
type AuthStatus = "checking" | "authenticated" | "unauthenticated" | "unknown";

interface OutputLine {
  id: number;
  text: string;
}

const MAX_OUTPUT_LINES = 50;
const CHECKING_INDICATOR_DELAY_MS = 2000;

interface AgentProviderCardProps {
  provider: ProviderDisplayInfo;
}

export function AgentProviderCard({ provider }: AgentProviderCardProps) {
  const { t } = useTranslation(["settings", "common"]);
  const isBuiltIn = provider.status === "built_in";
  const supportsInstall = provider.supportsInstall === true;
  const supportsAuth = provider.supportsAuth === true;
  const supportsAuthStatus = provider.supportsAuthStatus === true;
  const hasBinary = !!provider.binaryName;
  const [setupPhase, setSetupPhase] = useState<SetupPhase>("idle");
  const [setupOutput, setSetupOutput] = useState<OutputLine[]>([]);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [showCheckingIndicator, setShowCheckingIndicator] = useState(false);
  const [installStatus, setInstallStatus] = useState<InstallStatus>(
    hasBinary && !isBuiltIn ? "checking" : "installed",
  );
  const [authStatus, setAuthStatus] = useState<AuthStatus>(
    supportsAuthStatus && hasBinary && !isBuiltIn ? "checking" : "unknown",
  );
  const outputRef = useRef<HTMLDivElement>(null);
  const outputLengthRef = useRef(0);
  const lineCounterRef = useRef(0);
  const isMountedRef = useRef(true);
  const unlistenRef = useRef<(() => void) | null>(null);

  const icon = getProviderIcon(provider.id, "size-6");
  const isActive = setupPhase !== "idle";
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
        setInstallStatus(installed ? "installed" : "missing");
        if (installed && supportsAuthStatus) {
          return checkAgentAuth(provider.id).then((authenticated) => {
            if (!isMountedRef.current) return;
            setAuthStatus(authenticated ? "authenticated" : "unauthenticated");
          });
        }
        if (installed && !supportsAuthStatus) {
          setAuthStatus(getAuthHint() ? "authenticated" : "unknown");
        }
        if (!installed) {
          setAuthStatus("unknown");
          setAuthHint(false);
        }
      })
      .catch(() => {
        if (!isMountedRef.current) return;
        setInstallStatus("missing");
        setAuthStatus("unknown");
      });
  }, [
    getAuthHint,
    hasBinary,
    isBuiltIn,
    provider.id,
    provider.binaryName,
    supportsAuthStatus,
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

    if (supportsInstall && installStatus === "missing") {
      await runInstall();
    } else if (supportsAuth) {
      await runAuth();
    }
  }

  async function runInstall() {
    if (!supportsInstall) return;
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
        const installed = await checkAgentInstalled(provider.id);
        if (!isMountedRef.current) return;
        setInstallStatus(installed ? "installed" : "missing");
        if (!installed) {
          setAuthStatus("unknown");
          setAuthHint(false);
          setSetupError(t("providers.agents.errors.installVerificationFailed"));
          setSetupPhase("idle");
          return;
        }
      }

      if (supportsAuth) {
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
    if (!supportsAuth) return;
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
      setAuthStatus("authenticated");
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

  const isReady =
    isBuiltIn ||
    (installStatus === "installed" && !supportsAuth) ||
    (installStatus === "installed" && authStatus === "authenticated");
  const needsAuth =
    installStatus === "installed" &&
    supportsAuth &&
    authStatus !== "checking" &&
    authStatus !== "authenticated";
  const needsInstall = installStatus === "missing" && supportsInstall;
  const isChecking =
    (installStatus === "checking" && hasBinary) ||
    (installStatus === "installed" && authStatus === "checking");

  useEffect(() => {
    if (!isChecking) {
      setShowCheckingIndicator(false);
      return;
    }

    setShowCheckingIndicator(false);
    const timeoutId = window.setTimeout(() => {
      if (isMountedRef.current) {
        setShowCheckingIndicator(true);
      }
    }, CHECKING_INDICATOR_DELAY_MS);

    return () => window.clearTimeout(timeoutId);
  }, [isChecking]);

  if (provider.showOnlyWhenInstalled && installStatus !== "installed")
    return null;

  function renderStatusIndicator() {
    if (isBuiltIn || isReady) {
      return (
        <div className="flex h-6 flex-shrink-0 items-center">
          <IconCheck className="size-4 text-success duration-200 motion-safe:animate-in motion-safe:fade-in" />
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

    if ((isChecking && showCheckingIndicator) || isActive) {
      return (
        <div
          role="status"
          aria-label={
            isChecking
              ? t("providers.agents.status.checking")
              : t("providers.agents.status.inProgress")
          }
          className="flex h-6 flex-shrink-0 items-center"
        >
          <Spinner
            role="presentation"
            aria-hidden="true"
            className="size-4 text-foreground"
          />
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
    if (setupError) return t("providers.agents.status.setupFailed");

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

  function renderSetupOutput(scrollToEnd = false) {
    if (setupOutput.length === 0) return null;

    return (
      <div
        ref={scrollToEnd ? outputRef : undefined}
        className="max-h-24 overflow-y-auto rounded-md bg-muted px-2 py-1.5 font-mono text-xxs leading-relaxed text-muted-foreground"
      >
        {setupOutput.map((entry) => (
          <div key={entry.id}>{entry.text || "\u00A0"}</div>
        ))}
      </div>
    );
  }

  function renderSetupProgress() {
    if (!isActive) return null;

    const phaseLabel =
      setupPhase === "installing"
        ? t("providers.agents.progress.installing", {
            name: provider.displayName,
          })
        : setupPhase === "authenticating"
          ? t("providers.waitingForSignIn")
          : t("providers.agents.progress.verifyingInstallation");

    const stepInfo =
      setupPhase === "installing" && supportsAuth
        ? t("providers.agents.progress.step", { step: 1, total: 2 })
        : setupPhase === "authenticating" && supportsInstall
          ? t("providers.agents.progress.step", { step: 2, total: 2 })
          : null;

    return (
      <div className="mt-3 space-y-2 border-t pt-3">
        <div className="flex items-center gap-2">
          <Spinner className="size-3.5 text-brand" />
          <div className="min-w-0 flex-1">
            <span className="text-xs font-medium">{phaseLabel}</span>
            {stepInfo && (
              <span className="ml-2 text-xxs text-muted-foreground">
                {stepInfo}
              </span>
            )}
          </div>
        </div>

        {renderSetupOutput(true)}
      </div>
    );
  }

  const statusText = renderStatusText();
  const action = renderAction();

  return (
    <div
      className={cn(
        "flex flex-col rounded-lg border bg-background p-3 transition-colors",
        isActive && "border-brand/50 bg-brand/10",
      )}
    >
      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          {icon ? (
            <div className="flex size-6 items-center justify-center [&>*]:size-6">
              {icon}
            </div>
          ) : null}
          <span className={cn("block text-sm", icon && "mt-2")}>
            {provider.displayName}
          </span>
          <p className="mt-1 text-xs text-muted-foreground">
            {provider.description}
          </p>
        </div>
        {renderStatusIndicator()}
      </div>

      {(statusText || action) && (
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
                        ? "bg-brand animate-pulse"
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
      )}

      {renderSetupProgress()}

      {setupError && !isActive && (
        <div className="mt-3 space-y-2 border-t pt-3">
          <p className="text-xs text-danger">{setupError}</p>
          {renderSetupOutput()}
        </div>
      )}
    </div>
  );
}
