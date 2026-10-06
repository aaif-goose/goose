import type { UserContent } from "ai";
import type { Message } from "discord.js";

export function redactSecrets(text: string): string {
  return text
    .replace(
      /\b((?:[\w-]+[_-])?(?:api[_-]?key|access[_-]?token|token|secret|password|authorization)["']?[ \t]*[:=][ \t]*)(["']?)(?:Bearer[ \t]+)?[^\s,"']+\2/gi,
      "$1$2[redacted]$2",
    )
    .replace(/\bBearer\s+[\w.-]+/gi, "Bearer [redacted]")
    .replace(/\bsk-(?:proj-|ant-)?[\w-]{16,}/g, "[redacted]");
}

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
      content.push({ type: "image", image: new URL(attachment.url) });
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
      } catch {
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
