import type { z } from 'zod';

export function parsePayload<S extends z.ZodType>(schema: S, payload: unknown): z.output<S> {
  const result = schema.safeParse(payload ?? {});
  if (result.success) {
    return result.data;
  }

  const issue = result.error.issues[0];
  const path = issue.path.join('.');
  throw new Error(
    path ? `Invalid "${path}": ${issue.message}` : `Invalid payload: ${issue.message}`
  );
}
