export function redactSecrets(text: string): string {
  return text
    .replace(
      /\b((?:[\w-]+[_-])?(?:api[_-]?key|access[_-]?token|token|secret|password|authorization)["']?[ \t]*[:=][ \t]*)(["']?)(?:Bearer[ \t]+)?[^\s,"']+\2/gi,
      "$1$2[redacted]$2",
    )
    .replace(/\bBearer\s+[\w.-]+/gi, "Bearer [redacted]")
    .replace(/\bsk-(?:proj-|ant-)?[\w-]{16,}/g, "[redacted]");
}
