import { compareVersions, satisfies } from 'compare-versions';
import { isHostPermission } from './hostCapabilities/permissions';
import type {
  ChatActionContribution,
  ClientExtensionManifest,
  ContentSuffixContribution,
  CustomRenderContribution,
  CustomRenderMatch,
  RootLinkContribution,
  SidecarContribution,
  ThemeContribution,
} from './types';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isSafeExtensionId(id: string): boolean {
  return /^[a-zA-Z0-9_-]+$/.test(id);
}

function parseChatActions(raw: unknown): ChatActionContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const chatActions = raw
    .map((entry) => {
      if (!isRecord(entry)) {
        return null;
      }
      if (typeof entry.id !== 'string' || typeof entry.label !== 'string') {
        return null;
      }
      return {
        id: entry.id,
        label: entry.label,
        when: typeof entry.when === 'string' ? entry.when : undefined,
      };
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return chatActions.length > 0 ? chatActions : undefined;
}

function parseRootLinks(raw: unknown): RootLinkContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const rootLinks = raw
    .map((entry) => {
      if (!isRecord(entry)) {
        return null;
      }
      if (typeof entry.id !== 'string' || typeof entry.label !== 'string') {
        return null;
      }
      return {
        id: entry.id,
        label: entry.label,
        when: typeof entry.when === 'string' ? entry.when : undefined,
      };
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return rootLinks.length > 0 ? rootLinks : undefined;
}

function parseContentSuffixes(raw: unknown): ContentSuffixContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const contentSuffixes = raw
    .map((entry) => {
      if (!isRecord(entry)) {
        return null;
      }
      if (typeof entry.id !== 'string') {
        return null;
      }
      return {
        id: entry.id,
        when: typeof entry.when === 'string' ? entry.when : undefined,
      };
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return contentSuffixes.length > 0 ? contentSuffixes : undefined;
}

function parseCustomRenderMatch(raw: unknown): CustomRenderMatch | null {
  if (!isRecord(raw)) {
    return null;
  }

  const match: CustomRenderMatch = {};

  if (typeof raw.contentType === 'string') {
    if (raw.contentType === 'code' || raw.contentType === 'text') {
      match.contentType = raw.contentType;
    }
  }

  if (typeof raw.language === 'string' && raw.language.trim()) {
    match.language = raw.language.trim().toLowerCase();
  }

  return Object.keys(match).length > 0 ? match : null;
}

function parseCustomRenders(raw: unknown): CustomRenderContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const customRenders = raw
    .map((entry) => {
      if (!isRecord(entry)) {
        return null;
      }
      if (typeof entry.id !== 'string') {
        return null;
      }

      const match = parseCustomRenderMatch(entry.match);
      if (!match) {
        return null;
      }

      const render: CustomRenderContribution = {
        id: entry.id,
        match,
      };

      if (typeof entry.when === 'string') {
        render.when = entry.when;
      }
      if (entry.display === 'inline') {
        render.display = 'inline';
      }
      if (typeof entry.priority === 'number' && Number.isFinite(entry.priority)) {
        render.priority = entry.priority;
      }

      return render;
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return customRenders.length > 0 ? customRenders : undefined;
}

function parseSidecars(raw: unknown): SidecarContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const sidecars = raw
    .map((entry) => {
      if (!isRecord(entry)) {
        return null;
      }
      if (typeof entry.id !== 'string' || typeof entry.label !== 'string') {
        return null;
      }

      const sidecar: SidecarContribution = {
        id: entry.id,
        label: entry.label,
      };

      if (typeof entry.when === 'string') {
        sidecar.when = entry.when;
      }
      if (entry.defaultOpen === true) {
        sidecar.defaultOpen = true;
      }

      return sidecar;
    })
    .filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return sidecars.length > 0 ? sidecars : undefined;
}

function parseThemes(raw: unknown): ThemeContribution[] | undefined {
  if (!Array.isArray(raw)) {
    return undefined;
  }

  const themes = raw
    .map((entry): ThemeContribution | null => {
      if (!isRecord(entry) || !isRecord(entry.tokens)) {
        return null;
      }
      if (typeof entry.id !== 'string' || !isSafeExtensionId(entry.id)) {
        return null;
      }
      if (typeof entry.label !== 'string' || !entry.label.trim()) {
        return null;
      }
      if (entry.variant !== 'light' && entry.variant !== 'dark') {
        return null;
      }

      const tokens: Record<string, string> = {};
      for (const [key, value] of Object.entries(entry.tokens)) {
        if (typeof value === 'string') {
          tokens[key] = value;
        }
      }

      return { id: entry.id, label: entry.label.trim(), variant: entry.variant, tokens };
    })
    .filter((entry): entry is ThemeContribution => entry !== null);

  return themes.length > 0 ? themes : undefined;
}

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

  if (isRecord(raw.contributes)) {
    const chatActions = parseChatActions(raw.contributes.chatActions);
    const rootLinks = parseRootLinks(raw.contributes.rootLinks);
    const contentSuffixes = parseContentSuffixes(raw.contributes.contentSuffixes);
    const customRenders = parseCustomRenders(raw.contributes.customRenders);
    const sidecars = parseSidecars(raw.contributes.sidecars);
    const themes = parseThemes(raw.contributes.themes);
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
