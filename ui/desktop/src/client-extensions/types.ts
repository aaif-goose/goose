import type { HostPermission } from './hostCapabilities/permissions';

export interface ChatActionContribution {
  id: string;
  label: string;
  when?: string;
}

export interface RootLinkContribution {
  id: string;
  label: string;
  when?: string;
}

export interface ContentSuffixContribution {
  id: string;
  when?: string;
}

export interface CustomRenderMatch {
  contentType?: 'code' | 'text';
  language?: string;
}

export interface CustomRenderContribution {
  id: string;
  match: CustomRenderMatch;
  display?: 'inline';
  priority?: number;
  when?: string;
}

export interface SidecarContribution {
  id: string;
  label: string;
  when?: string;
  defaultOpen?: boolean;
}

export interface ThemeContribution {
  id: string;
  label: string;
  variant: 'light' | 'dark';
  tokens: Record<string, string>;
}

export interface ClientExtensionContributes {
  chatActions?: ChatActionContribution[];
  rootLinks?: RootLinkContribution[];
  contentSuffixes?: ContentSuffixContribution[];
  customRenders?: CustomRenderContribution[];
  sidecars?: SidecarContribution[];
  themes?: ThemeContribution[];
}

export interface ClientExtensionManifest {
  id: string;
  version: string;
  engines?: {
    grc?: string;
  };
  main: string;
  permissions?: HostPermission[];
  contributes?: ClientExtensionContributes;
}

export interface DiscoveredClientExtension {
  id: string;
  manifest: ClientExtensionManifest;
  source: ClientExtensionSource;
  enabled: boolean;
}

export type ClientExtensionSource = 'installed' | 'dev';

export interface RegisteredChatAction extends ChatActionContribution {
  extensionId: string;
}

export interface RegisteredRootLink extends RootLinkContribution {
  extensionId: string;
  path: string;
}

export interface RegisteredContentSuffix extends ContentSuffixContribution {
  extensionId: string;
}

export interface RegisteredCustomRender extends CustomRenderContribution {
  extensionId: string;
}

export interface RegisteredSidecar extends SidecarContribution {
  extensionId: string;
}

export interface RegisteredTheme extends ThemeContribution {
  extensionId: string;
}

export interface CodeBlock {
  language: string;
  content: string;
}

export interface ExtensionHostContext {
  sessionId: string | null;
  route: string;
}

export interface MessageExtensionHostContext extends ExtensionHostContext {
  messageId: string | null;
  role: string;
  hasText: boolean;
  hasImage: boolean;
  hasToolRequests: boolean;
  codeLanguages: string[];
}

export interface MessageRenderPayload {
  textPreview: string;
  codeBlocks: CodeBlock[];
  matchedLanguage?: string;
}

export type HostToExtensionMessage =
  | {
      type: 'grc/action';
      actionId: string;
      context: ExtensionHostContext;
    }
  | {
      type: 'grc/activate';
      viewId: string;
      viewKind?: 'rootLink' | 'sidecar';
      context: ExtensionHostContext;
    }
  | {
      type: 'grc/render';
      slotId: string;
      slotKind: 'contentSuffix' | 'customRender';
      context: MessageExtensionHostContext;
      payload: MessageRenderPayload;
    };
