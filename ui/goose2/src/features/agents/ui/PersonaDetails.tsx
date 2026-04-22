import { useTranslation } from "react-i18next";
import {
  Avatar as AvatarRoot,
  AvatarFallback,
  AvatarImage,
} from "@/shared/ui/avatar";
import { Badge } from "@/shared/ui/badge";
import { MessageResponse } from "@/shared/ui/ai-elements/message";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import type { Avatar } from "@/shared/types/agents";
import type { PersonaSource } from "@/features/agents/lib/personaPresentation";

interface PersonaDetailsProps {
  avatar: Avatar | null;
  displayName: string;
  modelLabel: string;
  personaSource: PersonaSource;
  providerLabel: string;
  systemPrompt: string;
}

export function PersonaDetails({
  avatar,
  displayName,
  modelLabel,
  personaSource,
  providerLabel,
  systemPrompt,
}: PersonaDetailsProps) {
  const { t } = useTranslation(["agents", "common"]);
  const avatarSrc = useAvatarSrc(avatar);
  const initials = displayName.charAt(0).toUpperCase() || "?";

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5">
      <div className="space-y-4">
        <section className="rounded-xl border border-border bg-muted/20 p-4">
          <div className="flex items-start gap-4">
            <AvatarRoot className="h-16 w-16 border border-border bg-background">
              <AvatarImage
                src={avatarSrc ?? undefined}
                alt={t("avatar.previewAlt")}
              />
              <AvatarFallback className="text-lg font-semibold">
                {initials}
              </AvatarFallback>
            </AvatarRoot>
            <div className="min-w-0 flex-1 space-y-2">
              <div className="space-y-1">
                <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
                  {t("editor.displayName")}
                </p>
                <h2 className="text-base font-semibold tracking-tight">
                  {displayName}
                </h2>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {personaSource === "builtin" ? (
                  <Badge variant="secondary">
                    {t("common:labels.builtIn")}
                  </Badge>
                ) : null}
                {personaSource === "file" ? (
                  <Badge variant="secondary">{t("card.fileBacked")}</Badge>
                ) : null}
              </div>
            </div>
          </div>
        </section>

        <section className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-2 rounded-xl border border-border bg-background p-4">
            <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
              {t("editor.provider")}
            </p>
            <p className="text-sm font-medium text-foreground">
              {providerLabel}
            </p>
          </div>
          <div className="space-y-2 rounded-xl border border-border bg-background p-4">
            <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
              {t("editor.model")}
            </p>
            <p className="text-sm font-medium text-foreground">{modelLabel}</p>
          </div>
        </section>

        <section className="space-y-2 rounded-xl border border-border bg-background p-4">
          <div className="flex items-center justify-between gap-3">
            <p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
              {t("editor.systemPrompt")}
            </p>
            <span className="text-[10px] text-muted-foreground">
              {t("common:labels.characterCount", {
                count: systemPrompt.length,
              })}
            </span>
          </div>
          <div className="rounded-lg border border-border bg-muted/20 px-4 py-3">
            <MessageResponse className="min-w-0 text-sm leading-6">
              {systemPrompt}
            </MessageResponse>
          </div>
        </section>
      </div>
    </div>
  );
}
