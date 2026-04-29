import { getClient } from "@/shared/api/acpConnection";
import type {
  CustomProviderCreateResponse,
  CustomProviderDeleteResponse,
  CustomProviderFormat,
  CustomProviderReadResponse,
  CustomProviderUpdateResponse,
  CustomProviderUpsertRequest,
  ProviderCatalogEntryDto,
  ProviderTemplateDto,
} from "../lib/customProviderTypes";

interface ProviderCatalogListRequest {
  format?: CustomProviderFormat;
}

interface ProviderCatalogListResponse {
  providers: ProviderCatalogEntryDto[];
}

interface ProviderCatalogTemplateRequest {
  providerId: string;
}

interface ProviderCatalogTemplateResponse {
  template: ProviderTemplateDto;
}

interface CustomProviderReadRequest {
  providerId: string;
}

interface CustomProviderUpdateRequest extends CustomProviderUpsertRequest {
  providerId: string;
}

interface CustomProviderDeleteRequest {
  providerId: string;
}

interface PlannedCustomProviderMethods {
  GooseProvidersCatalogList(
    params: ProviderCatalogListRequest,
  ): Promise<ProviderCatalogListResponse>;
  GooseProvidersCatalogTemplate(
    params: ProviderCatalogTemplateRequest,
  ): Promise<ProviderCatalogTemplateResponse>;
  GooseProvidersCustomCreate(
    params: CustomProviderUpsertRequest,
  ): Promise<CustomProviderCreateResponse>;
  GooseProvidersCustomRead(
    params: CustomProviderReadRequest,
  ): Promise<CustomProviderReadResponse>;
  GooseProvidersCustomUpdate(
    params: CustomProviderUpdateRequest,
  ): Promise<CustomProviderUpdateResponse>;
  GooseProvidersCustomDelete(
    params: CustomProviderDeleteRequest,
  ): Promise<CustomProviderDeleteResponse>;
}

async function getProviderClient(): Promise<PlannedCustomProviderMethods> {
  const client = await getClient();
  return client.goose as unknown as PlannedCustomProviderMethods;
}

export async function listCustomProviderCatalog(
  format?: CustomProviderFormat,
): Promise<ProviderCatalogEntryDto[]> {
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
  return client.GooseProvidersCustomUpdate({ providerId, ...input });
}

export async function deleteCustomProvider(
  providerId: string,
): Promise<CustomProviderDeleteResponse> {
  const client = await getProviderClient();
  return client.GooseProvidersCustomDelete({ providerId });
}
