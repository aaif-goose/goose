export type CspMetadata = {
  baseUriDomains?: string[] | null;
  connectDomains?: string[] | null;
  frameDomains?: string[] | null;
  resourceDomains?: string[] | null;
};

export type PermissionsMetadata = {
  camera?: boolean;
  clipboardWrite?: boolean;
  geolocation?: boolean;
  microphone?: boolean;
};

export type UiMetadata = {
  csp?: CspMetadata | null;
  domain?: string | null;
  permissions?: PermissionsMetadata;
  prefersBorder?: boolean | null;
};

export type ResourceMetadata = {
  ui?: UiMetadata | null;
};

export type McpAppResource = {
  _meta?: ResourceMetadata | null;
  blob?: string | null;
  description?: string | null;
  mimeType: string;
  name: string;
  text?: string | null;
  uri: string;
};

export type WindowProps = {
  height?: number;
  resizable?: boolean;
  width?: number;
};

export type GooseApp = McpAppResource &
  WindowProps & {
    mcpServers?: string[];
    prd?: string | null;
    deletable?: boolean;
  };
