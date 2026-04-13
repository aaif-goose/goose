import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  inspectAttachmentPaths,
  readImageAttachment,
} from "@/shared/api/system";
import type {
  ChatAttachmentDraft,
  ChatDirectoryAttachmentDraft,
  ChatFileAttachmentDraft,
  ChatImageAttachmentDraft,
} from "@/shared/types/messages";
import { getPlatform } from "@/shared/lib/platform";
import { resizeImage } from "../lib/resizeImage";

function isBlobPreview(url: string) {
  return url.startsWith("blob:");
}

function revokeAttachmentPreview(attachment: ChatAttachmentDraft) {
  if (attachment.kind === "image" && isBlobPreview(attachment.previewUrl)) {
    URL.revokeObjectURL(attachment.previewUrl);
  }
}

function pathToPreviewUrl(path: string) {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__
    ? convertFileSrc(path)
    : path;
}

function attachmentPathKey(path?: string) {
  if (!path) {
    return null;
  }

  return getPlatform() === "linux" ? path : path.toLowerCase();
}

async function createImageAttachmentFromFile(
  file: File,
): Promise<ChatImageAttachmentDraft> {
  const previewUrl = URL.createObjectURL(file);

  try {
    const { base64, mimeType } = await resizeImage(file).catch(
      () =>
        new Promise<{ base64: string; mimeType: string }>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => {
            const dataUrl = reader.result as string;
            const [header, base64] = dataUrl.split(",");
            const mimeType = header.replace("data:", "").replace(";base64", "");
            resolve({ base64, mimeType });
          };
          reader.onerror = () => reject(new Error("Failed to read image"));
          reader.readAsDataURL(file);
        }),
    );

    return {
      id: crypto.randomUUID(),
      kind: "image",
      name: file.name,
      mimeType,
      base64,
      previewUrl,
    };
  } catch (error) {
    URL.revokeObjectURL(previewUrl);
    throw error;
  }
}

export function normalizeDialogSelection(
  selected: string | string[] | null,
): string[] {
  if (!selected) {
    return [];
  }

  return Array.isArray(selected) ? selected : [selected];
}

export function useChatInputAttachments() {
  const [attachments, setAttachments] = useState<ChatAttachmentDraft[]>([]);
  const attachmentsRef = useRef(attachments);
  attachmentsRef.current = attachments;

  useEffect(
    () => () => {
      for (const attachment of attachmentsRef.current) {
        revokeAttachmentPreview(attachment);
      }
    },
    [],
  );

  const appendAttachments = useCallback((incoming: ChatAttachmentDraft[]) => {
    if (incoming.length === 0) {
      return;
    }

    setAttachments((previous) => {
      const seenPaths = new Set(
        previous
          .map((attachment) => attachmentPathKey(attachment.path))
          .filter((value): value is string => Boolean(value)),
      );
      const next = [...previous];

      for (const attachment of incoming) {
        const pathKey = attachmentPathKey(attachment.path);
        if (pathKey && seenPaths.has(pathKey)) {
          revokeAttachmentPreview(attachment);
          continue;
        }

        if (pathKey) {
          seenPaths.add(pathKey);
        }
        next.push(attachment);
      }

      return next;
    });
  }, []);

  const addBrowserFiles = useCallback(
    async (files: File[]) => {
      const nextAttachments = await Promise.all(
        files.map(async (file) => {
          if (file.type.startsWith("image/")) {
            return createImageAttachmentFromFile(file);
          }

          return {
            id: crypto.randomUUID(),
            kind: "file",
            name: file.name,
            ...(file.type ? { mimeType: file.type } : {}),
          } satisfies ChatFileAttachmentDraft;
        }),
      );

      appendAttachments(nextAttachments);
    },
    [appendAttachments],
  );

  const addPathAttachments = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) {
        return;
      }

      const inspectedPaths = await inspectAttachmentPaths(paths);
      const nextAttachments = await Promise.all(
        inspectedPaths.map(async (attachmentPath) => {
          if (attachmentPath.kind === "directory") {
            return {
              id: crypto.randomUUID(),
              kind: "directory",
              name: attachmentPath.name,
              path: attachmentPath.path,
            } satisfies ChatDirectoryAttachmentDraft;
          }

          if (attachmentPath.mimeType?.startsWith("image/")) {
            try {
              const image = await readImageAttachment(attachmentPath.path);
              return {
                id: crypto.randomUUID(),
                kind: "image",
                name: attachmentPath.name,
                path: attachmentPath.path,
                mimeType: image.mimeType,
                base64: image.base64,
                previewUrl: pathToPreviewUrl(attachmentPath.path),
              } satisfies ChatImageAttachmentDraft;
            } catch {
              // Fall back to a generic file attachment if image loading fails.
            }
          }

          return {
            id: crypto.randomUUID(),
            kind: "file",
            name: attachmentPath.name,
            path: attachmentPath.path,
            ...(attachmentPath.mimeType
              ? { mimeType: attachmentPath.mimeType }
              : {}),
          } satisfies ChatFileAttachmentDraft;
        }),
      );

      appendAttachments(nextAttachments);
    },
    [appendAttachments],
  );

  const removeAttachment = useCallback((id: string) => {
    setAttachments((previous) => {
      const found = previous.find((attachment) => attachment.id === id);
      if (found) {
        revokeAttachmentPreview(found);
      }
      return previous.filter((attachment) => attachment.id !== id);
    });
  }, []);

  const clearAttachments = useCallback(() => {
    setAttachments((previous) => {
      for (const attachment of previous) {
        revokeAttachmentPreview(attachment);
      }
      return [];
    });
  }, []);

  return {
    attachments,
    addBrowserFiles,
    addPathAttachments,
    removeAttachment,
    clearAttachments,
  };
}
