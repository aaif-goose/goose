import { useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Copy,
  CopyPlus,
  MessageSquarePlus,
  MoreVertical,
  Pencil,
  Save,
  Share2,
  Trash2,
} from "lucide-react";
import { MessageResponse } from "@/shared/ui/ai-elements/message";
import {
  Avatar as AvatarRoot,
  AvatarFallback,
  AvatarImage,
} from "@/shared/ui/avatar";
import { Button } from "@/shared/ui/button";
import { DetailField } from "@/shared/ui/detail-field";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { PageColumns } from "@/shared/ui/page-columns";
import { DetailPageShell, PageHeader } from "@/shared/ui/page-shell";
import { useAvatarSrc } from "@/shared/hooks/useAvatarSrc";
import type { Persona } from "@/shared/types/agents";
import { getPersonaInitials } from "@/features/agents/lib/personaPresentation";

interface AgentDetailPageProps {
  persona: Persona;
  onBack: () => void;
  onEdit: (persona: Persona) => void;
  onStartChat?: (persona: Persona) => void;
  onCopyFile: (persona: Persona) => void;
  onSaveCopy: (persona: Persona) => void;
  onDuplicate: (persona: Persona) => void;
  onDelete: (persona: Persona) => void;
}

interface AgentHeaderActionButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  icon: ReactNode;
}

function AgentHeaderActionButton({
  label,
  icon,
  type = "button",
  ...props
}: AgentHeaderActionButtonProps) {
  return (
    <Button
      type={type}
      size="xs"
      variant="outline-flat"
      leftIcon={icon}
      {...props}
    >
      {label}
    </Button>
  );
}

export function AgentDetailPage({
  persona,
  onBack,
  onEdit,
  onStartChat,
  onCopyFile,
  onSaveCopy,
  onDuplicate,
  onDelete,
}: AgentDetailPageProps) {
  const { t } = useTranslation(["agents", "common"]);
  const [menuOpen, setMenuOpen] = useState(false);
  const avatarSrc = useAvatarSrc(persona.avatar);
  const initials = getPersonaInitials(persona.displayName);
  const hasFileActions = Boolean(persona.sourcePath);
  const providerLabel = persona.provider || t("common:labels.none");
  const modelLabel = persona.model || t("common:labels.none");
  const shareLabel = t("view.share");
  const moreLabel = t("view.more");

  return (
    <DetailPageShell>
      <div className="space-y-5 border-b border-border pb-6">
        <Button
          type="button"
          variant="back"
          size="sm"
          className="w-fit"
          onClick={onBack}
        >
          {t("view.backToAgents")}
        </Button>

        <PageHeader
          variant="detail"
          title={
            <span className="inline-flex min-w-0 items-center gap-3">
              <AvatarRoot className="size-12 shrink-0 border border-border-soft bg-muted/30">
                <AvatarImage
                  src={avatarSrc ?? undefined}
                  alt={persona.displayName}
                />
                <AvatarFallback className="text-base font-semibold">
                  {initials}
                </AvatarFallback>
              </AvatarRoot>
              <span className="min-w-0 truncate">{persona.displayName}</span>
            </span>
          }
          description={persona.systemPrompt}
          descriptionClassName="line-clamp-2 max-w-3xl leading-relaxed"
          actionsPlacement="below"
          actions={
            <>
              {onStartChat ? (
                <AgentHeaderActionButton
                  label={t("view.startChatShort")}
                  icon={<MessageSquarePlus aria-hidden="true" />}
                  onClick={() => onStartChat(persona)}
                />
              ) : null}
              <AgentHeaderActionButton
                label={t("common:actions.edit")}
                icon={<Pencil aria-hidden="true" />}
                onClick={() => onEdit(persona)}
              />
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="xs"
                    variant="outline-flat"
                    leftIcon={<Share2 aria-hidden="true" />}
                    disabled={!hasFileActions}
                  >
                    {shareLabel}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" sideOffset={8}>
                  <DropdownMenuItem onSelect={() => onCopyFile(persona)}>
                    <Copy className="size-3.5" />
                    {t("view.copyFile")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => onSaveCopy(persona)}>
                    <Save className="size-3.5" />
                    {t("view.saveCopy")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    size="icon-xs"
                    variant="outline-flat"
                    aria-label={moreLabel}
                  >
                    <MoreVertical className="size-3.5" />
                    <span className="sr-only">{moreLabel}</span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" sideOffset={8}>
                  <DropdownMenuItem onSelect={() => onDuplicate(persona)}>
                    <CopyPlus className="size-3.5" />
                    {t("editor.duplicate")}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    variant="destructive"
                    onSelect={() => onDelete(persona)}
                  >
                    <Trash2 className="size-3.5" />
                    {t("common:actions.delete")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </>
          }
          actionsClassName="gap-2"
        />
      </div>

      <PageColumns
        defaultSidebarSize={30}
        minSidebarSize={24}
        maxSidebarSize={38}
        minContentSize={52}
        sidebar={
          <aside className="space-y-5">
            <section className="space-y-5">
              <DetailField
                label={t("editor.provider")}
                contentAs="p"
                contentClassName="break-words"
              >
                {providerLabel}
              </DetailField>

              <DetailField
                label={t("editor.model")}
                contentAs="p"
                contentClassName="break-words"
              >
                {modelLabel}
              </DetailField>
            </section>
          </aside>
        }
      >
        <section className="space-y-4 pb-6">
          <DetailField
            label={t("editor.systemPrompt")}
            meta={
              <span className="text-[10px] text-muted-foreground">
                {t("common:labels.characterCount", {
                  count: persona.systemPrompt.length,
                })}
              </span>
            }
          />
          <MessageResponse className="min-w-0 text-sm leading-6">
            {persona.systemPrompt || " "}
          </MessageResponse>
        </section>
      </PageColumns>
    </DetailPageShell>
  );
}
