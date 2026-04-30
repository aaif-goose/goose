import type { ChatAttachmentDraft } from "@/shared/types/messages";

export const DEFAULT_CHAT_TITLE = "New chat";
const ACP_DEFAULT_CHAT_TITLE = "New Chat";

export function isDefaultChatTitle(title: string): boolean {
  return title === DEFAULT_CHAT_TITLE || title === ACP_DEFAULT_CHAT_TITLE;
}

function attachmentKindLabel(kind: ChatAttachmentDraft["kind"], count: number) {
  switch (kind) {
    case "image":
      return count === 1 ? "image" : "images";
    case "directory":
      return count === 1 ? "folder" : "folders";
    default:
      return count === 1 ? "file" : "files";
  }
}

// The goose ACP backend uses "New Chat" (title case) as its default — normalize to ours.
export function normalizeAcpTitle(
  title: string | null | undefined,
): string | undefined {
  if (!title) return undefined;
  return title === ACP_DEFAULT_CHAT_TITLE ? DEFAULT_CHAT_TITLE : title;
}

export function getSessionTitleFromDraft(
  text: string,
  attachments?: ChatAttachmentDraft[],
): string {
  const trimmed = text.trim();
  if (trimmed.length > 0) {
    return trimmed.slice(0, 100);
  }

  if (!attachments || attachments.length === 0) {
    return DEFAULT_CHAT_TITLE;
  }

  const firstKind = attachments[0]?.kind;
  const sameKind = attachments.every(
    (attachment) => attachment.kind === firstKind,
  );
  const kindLabel = sameKind
    ? attachmentKindLabel(firstKind, attachments.length)
    : "files";

  return `Attached ${kindLabel}`;
}

export function getDisplaySessionTitle(
  title: string,
  defaultTitle: string,
): string {
  return isDefaultChatTitle(title) ? defaultTitle : title;
}

export function getEditableSessionTitle(
  title: string,
  defaultTitle: string,
): string {
  return getDisplaySessionTitle(title, defaultTitle);
}

export function isSessionTitleUnchanged(
  nextTitle: string,
  currentTitle: string,
  defaultTitle: string,
): boolean {
  return nextTitle === getEditableSessionTitle(currentTitle, defaultTitle);
}
