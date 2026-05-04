import { useTranslation } from "react-i18next";
import {
  Avatar as AvatarRoot,
  AvatarFallback,
  AvatarImage,
} from "@/shared/ui/avatar";
import { DetailField } from "@/shared/ui/detail-field";
import { MessageResponse } from "@/shared/ui/ai-elements/message";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import type { Avatar } from "@/shared/types/agents";
import { getPersonaInitials } from "@/features/agents/lib/personaPresentation";

interface PersonaDetailsProps {
  avatar: Avatar | null;
  displayName: string;
  modelLabel: string;
  providerLabel: string;
  systemPrompt: string;
}

export function PersonaDetails({
  avatar,
  displayName,
  modelLabel,
  providerLabel,
  systemPrompt,
}: PersonaDetailsProps) {
  const { t } = useTranslation(["agents", "common"]);
  const avatarSrc = useAvatarSrc(avatar);
  const initials = getPersonaInitials(displayName);

  return (
    <div className="space-y-4 px-5 pb-5">
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
            <DetailField
              label={t("editor.displayName")}
              contentClassName="text-base font-semibold tracking-tight"
            >
              {displayName}
            </DetailField>
          </div>
        </div>
      </section>

      <section className="grid gap-3 sm:grid-cols-2">
        <div className="rounded-xl border border-border bg-background p-4">
          <DetailField
            label={t("editor.provider")}
            contentClassName="font-medium"
          >
            {providerLabel}
          </DetailField>
        </div>
        <div className="rounded-xl border border-border bg-background p-4">
          <DetailField label={t("editor.model")} contentClassName="font-medium">
            {modelLabel}
          </DetailField>
        </div>
      </section>

      <section className="space-y-2 rounded-xl border border-border bg-background p-4">
        <DetailField
          label={t("editor.systemPrompt")}
          meta={
            <span className="text-[10px] text-muted-foreground">
              {t("common:labels.characterCount", {
                count: systemPrompt.length,
              })}
            </span>
          }
        />
        <div className="rounded-lg border border-border bg-muted/20 px-4 py-3">
          <MessageResponse className="min-w-0 text-sm leading-6">
            {systemPrompt}
          </MessageResponse>
        </div>
      </section>
    </div>
  );
}
