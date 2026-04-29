import figureUrl from "@/assets/agents/figure.png";
import ralphUrl from "@/assets/agents/ralph.png";
import scoutUrl from "@/assets/agents/scout.png";
import soloUrl from "@/assets/agents/solo.png";
import tulsiUrl from "@/assets/agents/tulsi.png";

const PERSONA_FIGURES: Record<string, string> = {
  ralph: ralphUrl,
  scout: scoutUrl,
  solo: soloUrl,
  tulsi: tulsiUrl,
};

export function resolvePersonaFigure(displayName: string | null | undefined) {
  return displayName
    ? (PERSONA_FIGURES[displayName.toLowerCase()] ?? figureUrl)
    : figureUrl;
}
