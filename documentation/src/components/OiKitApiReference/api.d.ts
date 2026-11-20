export type LanguageId = "rust" | "python" | "kotlin";

export type Language = {
  id: LanguageId;
  label: string;
  prism: string;
  func: (name: string) => string;
  field: (name: string) => string;
  variant: (name: string, isDataCarrying: boolean) => string;
  type: (type: string) => string;
  default: (value: string) => string;
  errorType: (name: string) => string;
};

export type OiKitParam = {
  name: string;
  type: string;
  default: string | null;
  docs: string;
};

export type OiKitFunc = {
  name: string;
  docs: string;
  params: OiKitParam[];
  returns: string | null;
  throws: string | null;
  isAsync: boolean;
};

export type OiKitItem = {
  name: string;
  kind: "object" | "callback" | "record" | "enum" | "error";
  docs: string;
  fields: OiKitParam[];
  variants: { name: string; fields: OiKitParam[] }[];
  methods: OiKitFunc[];
};

export type OiKitApiDoc = {
  version: string;
  docVersion: string;
  source: string;
  functions: OiKitFunc[];
  items: OiKitItem[];
};

export const LANGUAGES: Language[];
export function slug(...parts: string[]): string;
export function funcAnchor(func: OiKitFunc, owner?: string): string;
export function itemAnchor(item: OiKitItem): string;
export function memberAnchor(
  ownerAnchor: string,
  kind: string,
  name: string,
): string;
export function signature(
  func: OiKitFunc,
  language: Language,
  owner?: string,
): string;
export function variantName(
  item: OiKitItem,
  variant: OiKitItem["variants"][number],
  language: Language,
): string;
