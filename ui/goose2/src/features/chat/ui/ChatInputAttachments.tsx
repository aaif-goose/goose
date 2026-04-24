import { useState } from "react";
import { FileText, FolderClosed, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ImageLightbox } from "@/shared/ui/ImageLightbox";
import type {
  ChatAttachmentDraft,
  ChatDirectoryAttachmentDraft,
  ChatFileAttachmentDraft,
  ChatImageAttachmentDraft,
} from "@/shared/types/messages";

function DraftImageAttachment({
  attachment,
  index,
  onRemove,
}: {
  attachment: ChatImageAttachmentDraft;
  index: number;
  onRemove: (id: string) => void;
}) {
  const [lightboxOpen, setLightboxOpen] = useState(false);
  const { t } = useTranslation("chat");

  return (
    <>
      <div className="group relative inline-block">
        <button
          type="button"
          onClick={() => setLightboxOpen(true)}
          className="block cursor-pointer rounded-lg"
          aria-label={t("attachments.view", { index: index + 1 })}
          title={attachment.path ?? attachment.name}
        >
          <img
            src={attachment.previewUrl}
            alt={t("attachments.alt", { index: index + 1 })}
            className="h-16 w-16 rounded-lg border border-border object-cover"
          />
        </button>
        <button
          type="button"
          onClick={() => onRemove(attachment.id)}
          className="absolute -right-1.5 -top-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-foreground text-background opacity-0 transition-opacity duration-150 group-hover:opacity-100"
          aria-label={t("attachments.remove")}
        >
          <X className="h-2.5 w-2.5" />
        </button>
      </div>
      <ImageLightbox
        src={attachment.previewUrl}
        alt={t("attachments.alt", { index: index + 1 })}
        open={lightboxOpen}
        onOpenChange={setLightboxOpen}
      />
    </>
  );
}

function DraftPathAttachment({
  attachment,
  onRemove,
}: {
  attachment: ChatFileAttachmentDraft | ChatDirectoryAttachmentDraft;
  onRemove: (id: string) => void;
}) {
  const { t } = useTranslation("chat");
  const Icon = attachment.kind === "directory" ? FolderClosed : FileText;

  return (
    <div
      className="group relative flex items-center gap-2 rounded-full border border-border bg-muted/40 px-3 py-1.5 pr-8 text-xs text-foreground"
      title={attachment.path ?? attachment.name}
    >
      <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      <span className="max-w-44 truncate">{attachment.name}</span>
      <button
        type="button"
        onClick={() => onRemove(attachment.id)}
        className="absolute right-2 flex h-4 w-4 items-center justify-center rounded-full text-muted-foreground opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:text-foreground"
        aria-label={t("attachments.remove")}
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
}

export function ChatInputAttachments({
  attachments,
  onRemove,
}: {
  attachments: ChatAttachmentDraft[];
  onRemove: (id: string) => void;
}) {
  if (attachments.length === 0) {
    return null;
  }

  return (
    <div className="mb-2 flex flex-wrap gap-2">
      {attachments.map((attachment, index) =>
        attachment.kind === "image" ? (
          <DraftImageAttachment
            key={attachment.id}
            attachment={attachment}
            index={index}
            onRemove={onRemove}
          />
        ) : (
          <DraftPathAttachment
            key={attachment.id}
            attachment={attachment}
            onRemove={onRemove}
          />
        ),
      )}
    </div>
  );
}
