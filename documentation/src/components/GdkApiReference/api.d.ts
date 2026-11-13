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

export type GdkParam = {
  name: string;
  type: string;
  default: string | null;
  docs: string;
};

export type GdkFunc = {
  name: string;
  docs: string;
  params: GdkParam[];
  returns: string | null;
  throws: string | null;
  isAsync: boolean;
};

export type GdkItem = {
  name: string;
  kind: "object" | "callback" | "record" | "enum" | "error";
  docs: string;
  fields: GdkParam[];
  variants: { name: string; fields: GdkParam[] }[];
  methods: GdkFunc[];
};

export type GdkApiDoc = {
  version: string;
  docVersion: string;
  source: string;
  functions: GdkFunc[];
  items: GdkItem[];
};

export const LANGUAGES: Language[];
export function slug(...parts: string[]): string;
export function funcAnchor(func: GdkFunc, owner?: string): string;
export function itemAnchor(item: GdkItem): string;
export function memberAnchor(
  ownerAnchor: string,
  kind: string,
  name: string,
): string;
export function signature(
  func: GdkFunc,
  language: Language,
  owner?: string,
): string;
export function variantName(
  item: GdkItem,
  variant: GdkItem["variants"][number],
  language: Language,
): string;
