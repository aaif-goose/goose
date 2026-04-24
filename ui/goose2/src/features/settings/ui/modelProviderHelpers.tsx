import type {
  ProviderField,
  ProviderFieldValue,
  ProviderSetupMethod,
} from "@/shared/types/providers";

export const MAX_SETUP_OUTPUT_LINES = 8;

export interface SetupOutputLine {
  id: number;
  text: string;
}

export function getDefaultFieldValue(field: ProviderField): ProviderFieldValue {
  return {
    key: field.key,
    value: null,
    isSet: false,
    isSecret: field.secret,
    required: field.required,
  };
}

export function resolveFieldValue(
  field: ProviderField,
  fieldValueMap: Map<string, ProviderFieldValue>,
): ProviderFieldValue {
  return fieldValueMap.get(field.key) ?? getDefaultFieldValue(field);
}

export function getDisplayValue(
  field: ProviderField,
  fieldValueMap: Map<string, ProviderFieldValue>,
  t: (key: string) => string,
): string {
  const fieldValue = resolveFieldValue(field, fieldValueMap);
  if (!fieldValue.isSet) return t("providers.models.notSet");
  return fieldValue.value ?? t("providers.saved");
}

export function createDraftValues(
  fields: ProviderField[],
  values: ProviderFieldValue[],
): Record<string, string> {
  const valueMap = new Map(values.map((value) => [value.key, value]));
  return Object.fromEntries(
    fields.map((field) => {
      const currentValue = valueMap.get(field.key);
      if (field.secret) {
        return [field.key, ""];
      }
      return [field.key, currentValue?.value ?? ""];
    }),
  );
}

export function getSetupMessage(
  setupMethod: ProviderSetupMethod,
  isConnected: boolean,
  supportsNativeConnect: boolean,
  t: (key: string) => string,
): string | null {
  if (isConnected) {
    switch (setupMethod) {
      case "oauth_device_code":
        return t("providers.models.setup.connected.oauthDeviceCode");
      case "oauth_browser":
        return t("providers.models.setup.connected.oauthBrowser");
      case "cloud_credentials":
        return t("providers.models.setup.connected.cloudCredentials");
      case "local":
        return t("providers.models.setup.connected.local");
      default:
        return null;
    }
  }

  switch (setupMethod) {
    case "oauth_browser":
    case "oauth_device_code":
      return supportsNativeConnect
        ? t("providers.models.setup.pending.oauthGuided")
        : t("providers.models.setup.pending.oauthTerminal");
    case "cloud_credentials":
      return t("providers.models.setup.pending.cloudCredentials");
    case "local":
      return t("providers.models.setup.pending.local");
    default:
      return null;
  }
}

export function getNativeConnectDescription(
  setupMethod: ProviderSetupMethod,
  t: (key: string) => string,
): string | null {
  switch (setupMethod) {
    case "oauth_device_code":
    case "oauth_browser":
      return t("providers.models.setup.nativeConnectDescription");
    default:
      return null;
  }
}

export function getFieldSetupDescription(
  setupMethod: ProviderSetupMethod,
  t: (key: string) => string,
): string | null {
  switch (setupMethod) {
    case "single_api_key":
      return t("providers.models.setup.fieldDescription.singleApiKey");
    case "config_fields":
      return t("providers.models.setup.fieldDescription.configFields");
    case "host_with_oauth_fallback":
      return t("providers.models.setup.fieldDescription.hostWithOauthFallback");
    case "cloud_credentials":
      return t("providers.models.setup.fieldDescription.cloudCredentials");
    default:
      return null;
  }
}

export function renderInlineCodeMessage(message: string) {
  const command = "`goose configure`";
  if (!message.includes(command)) {
    return <p className="text-xs text-muted-foreground">{message}</p>;
  }

  const [before, after] = message.split(command);
  return (
    <p className="text-xs text-muted-foreground">
      {before}
      <code className="rounded bg-muted px-1 py-0.5 text-xxs">
        goose configure
      </code>
      {after}
    </p>
  );
}

export function renderSetupMessage(message: string | null) {
  if (!message) {
    return null;
  }

  if (message.includes("`goose configure`")) {
    return renderInlineCodeMessage(message);
  }

  return <p className="text-xs text-muted-foreground">{message}</p>;
}
