import { getClient } from "@/shared/api/acpConnection";
import type {
  CustomProviderCreateResponse,
  CustomProviderDeleteResponse,
  CustomProviderReadResponse,
  CustomProviderUpdateResponse,
  ProviderTemplateCatalogEntryDto,
  ProviderTemplateDto,
} from "@aaif/goose-sdk";
import type {
  CustomProviderFormat,
  CustomProviderUpsertRequest,
} from "../lib/customProviderTypes";

async function getProviderClient() {
  const client = await getClient();
  return client.goose;
}

export async function listCustomProviderCatalog(
  format?: CustomProviderFormat,
): Promise<ProviderTemplateCatalogEntryDto[]> {
  const client = await getProviderClient();
  const response = await client.GooseProvidersCatalogList(
    format ? { format } : {},
  );
  return response.providers;
}

export async function getCustomProviderTemplate(
  providerId: string,
): Promise<ProviderTemplateDto> {
  const client = await getProviderClient();
  const response = await client.GooseProvidersCatalogTemplate({ providerId });
  return response.template;
}

export async function createCustomProvider(
  input: CustomProviderUpsertRequest,
): Promise<CustomProviderCreateResponse> {
  const client = await getProviderClient();
  return client.GooseProvidersCustomCreate(input);
}

export async function readCustomProvider(
  providerId: string,
): Promise<CustomProviderReadResponse> {
  const client = await getProviderClient();
  return client.GooseProvidersCustomRead({ providerId });
}

export async function updateCustomProvider(
  providerId: string,
  input: CustomProviderUpsertRequest,
): Promise<CustomProviderUpdateResponse> {
  const client = await getProviderClient();
  return client.GooseProvidersCustomUpdate({ ...input, providerId });
}

export async function deleteCustomProvider(
  providerId: string,
): Promise<CustomProviderDeleteResponse> {
  const client = await getProviderClient();
  return client.GooseProvidersCustomDelete({ providerId });
}
