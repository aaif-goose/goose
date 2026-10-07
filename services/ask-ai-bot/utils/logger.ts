import "dotenv/config";
import { JSONParseError, TypeValidationError } from "ai";
import { createConsola } from "consola";
import { redactSecrets } from "./redact-secrets";

export const logger = createConsola();

export function errorDetails(error: unknown, depth = 0): unknown {
  if (!(error instanceof Error))
    return { message: redactSecrets(String(error)) };
  const details = error as Error & { code?: string; statusCode?: number };
  return {
    name: error.name,
    message: JSONParseError.isInstance(error)
      ? "Model output could not be parsed as JSON"
      : TypeValidationError.isInstance(error)
        ? "Model output did not match the schema"
        : redactSecrets(error.message),
    code: details.code,
    statusCode: details.statusCode,
    cause:
      error.cause && depth < 3
        ? errorDetails(error.cause, depth + 1)
        : undefined,
    stack:
      logger.level >= 5
        ? error.stack?.split("\n").slice(1).join("\n")
        : undefined,
  };
}
