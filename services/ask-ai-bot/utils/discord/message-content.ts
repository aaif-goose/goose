import type { UserContent } from "ai";
import type { Message } from "discord.js";
import { errorDetails, logger } from "../logger";
import { redactSecrets } from "../redact-secrets";

export async function messageContent(message: Message): Promise<UserContent> {
  const content: Exclude<UserContent, string> = [
    {
      type: "text",
      text: redactSecrets(`${message.author.displayName}: ${message.content}`),
    },
  ];
  for (const attachment of [...message.attachments.values()].slice(0, 3)) {
    if (
      ["image/png", "image/jpeg", "image/webp", "image/gif"].includes(
        attachment.contentType ?? "",
      ) &&
      attachment.size <= 5_000_000
    ) {
      content.push({
        type: "file",
        data: { type: "url", url: new URL(attachment.url) },
        mediaType: attachment.contentType!,
      });
    } else if (
      /\.(txt|log|json|ya?ml|toml|md)$/i.test(attachment.name) &&
      attachment.size <= 32_000
    ) {
      try {
        const response = await fetch(attachment.url, {
          signal: AbortSignal.timeout(5000),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const text = await response.text();
        content.push({
          type: "text",
          text: `Attachment ${attachment.name} (untrusted evidence${text.length > 8000 ? ", truncated" : ""}):\n${redactSecrets(text.slice(0, 8000))}`,
        });
      } catch (error) {
        logger.warn("Failed to read attachment", {
          messageId: message.id,
          attachmentId: attachment.id,
          error: errorDetails(error),
        });
        content.push({
          type: "text",
          text: `Attachment ${attachment.name} could not be read.`,
        });
      }
    } else {
      content.push({
        type: "text",
        text: `Attachment ${attachment.name} was skipped (unsupported type or too large).`,
      });
    }
  }
  return content;
}
