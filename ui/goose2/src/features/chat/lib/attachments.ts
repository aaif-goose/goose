import type {
  ChatAttachmentDraft,
  MessageAttachment,
} from "@/shared/types/messages";

export function appendAttachmentPaths(
  text: string,
  attachments: ChatAttachmentDraft[] | undefined,
): string {
  const paths = (attachments ?? [])
    .filter((attachment) => attachment.kind !== "image" && attachment.path)
    .map((attachment) => attachment.path as string);

  if (paths.length === 0) {
    return text;
  }

  const joined = paths.join(" ");
  return text ? `${text} ${joined}` : joined;
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

    messageAttachments.push({
      type: "file",
      name: attachment.name,
      ...(attachment.path ? { path: attachment.path } : {}),
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
