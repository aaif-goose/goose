import type {
  ChatAttachmentDraft,
  MessageAttachment,
} from "@/shared/types/messages";

function formatAttachmentReference(attachment: ChatAttachmentDraft): string {
  const location =
    attachment.kind === "image"
      ? `${attachment.name} (image attached)`
      : (attachment.path ?? attachment.name);
  return `- [${attachment.kind}] ${location}`;
}

export function buildAttachmentPromptPreamble(
  attachments: ChatAttachmentDraft[] | undefined,
): string {
  const referencedAttachments = attachments ?? [];

  if (referencedAttachments.length === 0) {
    return "";
  }

  return [
    "Attached items:",
    ...referencedAttachments.map(formatAttachmentReference),
    "",
  ].join("\n");
}

export function buildMessageAttachments(
  attachments: ChatAttachmentDraft[] | undefined,
): MessageAttachment[] | undefined {
  const messageAttachments: MessageAttachment[] = [];

  for (const attachment of attachments ?? []) {
    if (attachment.kind === "directory") {
      messageAttachments.push({
        type: "directory",
        name: attachment.name,
        path: attachment.path,
      });
      continue;
    }

    if (!attachment.path) {
      continue;
    }

    messageAttachments.push({
      type: "file",
      name: attachment.name,
      path: attachment.path,
      ...(attachment.kind === "image" || attachment.mimeType
        ? { mimeType: attachment.mimeType }
        : {}),
    });
  }

  return messageAttachments.length > 0 ? messageAttachments : undefined;
}

export function buildAcpImages(
  attachments: ChatAttachmentDraft[] | undefined,
): { base64: string; mimeType: string }[] | undefined {
  const images = (attachments ?? []).flatMap((attachment) =>
    attachment.kind === "image"
      ? [{ base64: attachment.base64, mimeType: attachment.mimeType }]
      : [],
  );

  return images.length > 0 ? images : undefined;
}
