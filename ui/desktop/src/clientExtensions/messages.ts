import { z } from 'zod';

const showMessageSchema = z.object({
  type: z.literal('grc/ui/showMessage'),
  text: z.string(),
});

const setInputSchema = z.object({
  type: z.literal('grc/chat/setInput'),
  text: z.string(),
});

const resizeSchema = z.object({
  type: z.literal('grc/resize'),
  height: z.number(),
});

const hostInvokeSchema = z.object({
  type: z.literal('grc/host/invoke'),
  capability: z.string(),
  method: z.string(),
  id: z.string().optional(),
  payload: z.unknown().optional(),
});

const extensionToHostMessageSchema = z.discriminatedUnion('type', [
  showMessageSchema,
  setInputSchema,
  resizeSchema,
  hostInvokeSchema,
]);

export type ExtensionToHostMessage = z.infer<typeof extensionToHostMessageSchema>;
export type HostCapabilityInvokeMessage = z.infer<typeof hostInvokeSchema>;

export function parseExtensionToHostMessage(value: unknown): ExtensionToHostMessage | null {
  const result = extensionToHostMessageSchema.safeParse(value);
  return result.success ? result.data : null;
}
