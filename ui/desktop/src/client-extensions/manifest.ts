import { compareVersions, satisfies } from 'compare-versions';
import { z } from 'zod';
import { isHostPermission } from './hostCapabilities/permissions';
import type { ClientExtensionManifest } from './types';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isSafeExtensionId(id: string): boolean {
  return /^[a-zA-Z0-9_-]+$/.test(id);
}

function isValidOrigin(value: unknown): value is string {
  if (typeof value !== 'string') {
    return false;
  }
  try {
    const url = new URL(value);
    return (
      (url.protocol === 'http:' || url.protocol === 'https:') &&
      url.origin === value.replace(/\/$/, '')
    );
  } catch {
    return false;
  }
}

function parseContributionList<S extends z.ZodType>(
  raw: unknown,
  schema: S
): z.output<S>[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const entries = raw
    .map((entry) => schema.safeParse(entry))
    .filter((result) => result.success)
    .map((result) => result.data);

  return entries.length > 0 ? entries : undefined;
}

const idLabelWhenSchema = z.object({
  id: z.string(),
  label: z.string(),
  when: z.string().optional(),
});

const contentSuffixSchema = z.object({
  id: z.string(),
  when: z.string().optional(),
});

const customRenderMatchSchema = z
  .object({
    contentType: z.enum(['code', 'text']).optional().catch(undefined),
    language: z
      .string()
      .optional()
      .transform((value) => {
        const trimmed = value?.trim().toLowerCase();
        return trimmed ? trimmed : undefined;
      }),
  })
  .refine((match) => match.contentType !== undefined || match.language !== undefined);

const customRenderSchema = z.object({
  id: z.string(),
  match: customRenderMatchSchema,
  when: z.string().optional(),
  display: z.literal('inline').optional().catch(undefined),
  priority: z.number().finite().optional().catch(undefined),
});

const sidecarSchema = z.object({
  id: z.string(),
  label: z.string(),
  when: z.string().optional(),
  defaultOpen: z.literal(true).optional().catch(undefined),
});

const themeTokensSchema = z.record(z.string(), z.unknown()).transform((tokens) => {
  const strings: Record<string, string> = {};
  for (const [key, value] of Object.entries(tokens)) {
    if (typeof value === 'string') {
      strings[key] = value;
    }
  }
  return strings;
});

const themeSchema = z.object({
  id: z.string().refine(isSafeExtensionId),
  label: z.string().trim().min(1),
  variant: z.enum(['light', 'dark']),
  tokens: themeTokensSchema,
});

export function parseClientExtensionManifest(raw: unknown): ClientExtensionManifest | null {
  if (!isRecord(raw)) {
    return null;
  }

  const { id, version, main } = raw;
  if (typeof id !== 'string' || !id.trim() || !isSafeExtensionId(id.trim())) {
    return null;
  }
  if (typeof version !== 'string' || !version.trim()) {
    return null;
  }
  if (typeof main !== 'string' || !main.trim()) {
    return null;
  }

  const manifest: ClientExtensionManifest = {
    id: id.trim(),
    version: version.trim(),
    runtime: raw.runtime === 'native' ? 'native' : 'sandbox',
    main: main.trim(),
  };

  if (isRecord(raw.engines) && typeof raw.engines.grc === 'string') {
    manifest.engines = { grc: raw.engines.grc };
  }

  if (Array.isArray(raw.permissions)) {
    const permissions = [...new Set(raw.permissions.filter(isHostPermission))];
    if (permissions.length > 0) {
      manifest.permissions = permissions;
    }
  }

  if (Array.isArray(raw.network)) {
    const origins = [...new Set(raw.network.filter(isValidOrigin))];
    if (origins.length > 0) {
      manifest.network = origins;
    }
  }

  if (isRecord(raw.contributes)) {
    const chatActions = parseContributionList(raw.contributes.chatActions, idLabelWhenSchema);
    const rootLinks = parseContributionList(raw.contributes.rootLinks, idLabelWhenSchema);
    const contentSuffixes = parseContributionList(
      raw.contributes.contentSuffixes,
      contentSuffixSchema
    );
    const customRenders = parseContributionList(raw.contributes.customRenders, customRenderSchema);
    const sidecars = parseContributionList(raw.contributes.sidecars, sidecarSchema);
    const themes = parseContributionList(raw.contributes.themes, themeSchema);
    if (chatActions || rootLinks || contentSuffixes || customRenders || sidecars || themes) {
      manifest.contributes = {
        ...(chatActions ? { chatActions } : {}),
        ...(rootLinks ? { rootLinks } : {}),
        ...(contentSuffixes ? { contentSuffixes } : {}),
        ...(customRenders ? { customRenders } : {}),
        ...(sidecars ? { sidecars } : {}),
        ...(themes ? { themes } : {}),
      };
    }
  }

  return manifest;
}

export function satisfiesGrcEngine(manifest: ClientExtensionManifest, grcVersion: string): boolean {
  const constraint = manifest.engines?.grc;
  if (!constraint) {
    return true;
  }

  try {
    if (/^[<>=]/.test(constraint) || constraint.includes(' ')) {
      return satisfies(grcVersion, constraint);
    }
    return compareVersions(grcVersion, constraint) >= 0;
  } catch {
    console.warn(`[client-extensions] Invalid engines.grc constraint "${constraint}"`);
    return false;
  }
}
