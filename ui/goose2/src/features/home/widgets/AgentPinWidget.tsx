import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { resolvePersonaFigure } from "@/features/agents/lib/personaFigure";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import type { Persona } from "@/shared/types/agents";
import { useWidgetActivationGuard } from "./useWidgetActivationGuard";
import type { WidgetRenderProps } from "./types";

function getAgentId(state: Record<string, unknown> | undefined): string | null {
  return typeof state?.agentId === "string" ? state.agentId : null;
}

function resolvePersona(personas: Persona[], id: string | null) {
  const normalizedId = id?.toLowerCase();
  return (
    personas.find(
      (persona) =>
        persona.id === id ||
        (normalizedId && persona.displayName.toLowerCase() === normalizedId),
    ) ??
    personas.find((persona) => persona.isBuiltin) ??
    personas[0]
  );
}

export function AgentPinWidget({
  instance,
  shouldIgnoreActivation,
  onStartChatWithPersona,
}: WidgetRenderProps) {
  const { t } = useTranslation("home");
  const personas = useAgentStore((state) => state.personas);
  const persona = useMemo(
    () => resolvePersona(personas, getAgentId(instance.state)),
    [instance.state, personas],
  );
  const label = persona?.displayName ?? t("widgets.agentPin.fallbackName");
  const personaId = persona?.id ?? getAgentId(instance.state) ?? "goose";
  const figureSrc = resolvePersonaFigure(persona?.displayName ?? "Scout");
  const activationGuard = useWidgetActivationGuard(shouldIgnoreActivation);

  return (
    <button
      type="button"
      {...activationGuard.pointerHandlers}
      onClick={(event) => {
        if (activationGuard.shouldIgnoreActivation()) {
          event.preventDefault();
          activationGuard.clearIgnoredActivation();
          return;
        }
        onStartChatWithPersona?.(personaId);
      }}
      aria-label={t("widgets.agentPin.openAria", { name: label })}
      className="group flex h-full w-full appearance-none items-center justify-center border-0 bg-transparent p-0 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-[var(--color-accent)]"
    >
      <img
        src={figureSrc}
        alt=""
        aria-hidden="true"
        className="pointer-events-none h-full w-full select-none object-contain transition-transform group-hover:scale-[1.03]"
      />
    </button>
  );
}
