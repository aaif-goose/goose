// This file is auto-generated — do not edit manually.

export interface ExtMethodProvider {
  extMethod(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
}

import type {
  AddExtensionRequest,
  ArchiveSessionRequest,
  CheckSecretRequest,
  CheckSecretResponse,
  DeleteSessionRequest,
  ExportSessionRequest,
  ExportSessionResponse,
  GetExtensionsRequest,
  GetExtensionsResponse,
  GetToolsRequest,
  GetToolsResponse,
  ImportSessionRequest,
  ImportSessionResponse,
  ListProvidersRequest,
  ListProvidersResponse,
  ReadConfigRequest,
  ReadConfigResponse,
  ReadResourceRequest,
  ReadResourceResponse,
  RemoveConfigRequest,
  RemoveExtensionRequest,
  RemoveSecretRequest,
  UnarchiveSessionRequest,
  UpdateProviderRequest,
  UpdateProviderResponse,
  UpdateWorkingDirRequest,
  UpsertConfigRequest,
  UpsertSecretRequest,
} from './types.gen.js';
import {
  zCheckSecretResponse,
  zExportSessionResponse,
  zGetExtensionsResponse,
  zGetToolsResponse,
  zImportSessionResponse,
  zListProvidersResponse,
  zReadConfigResponse,
  zReadResourceResponse,
  zUpdateProviderResponse,
} from './zod.gen.js';

export class GooseExtClient {
  constructor(private conn: ExtMethodProvider) {}

  async GooseExtensionsAdd(params: AddExtensionRequest): Promise<void> {
    await this.conn.extMethod("_goose/extensions/add", params);
  }

  async GooseExtensionsRemove(params: RemoveExtensionRequest): Promise<void> {
    await this.conn.extMethod("_goose/extensions/remove", params);
  }

  async GooseTools(params: GetToolsRequest): Promise<GetToolsResponse> {
    const raw = await this.conn.extMethod("_goose/tools", params);
    return zGetToolsResponse.parse(raw) as GetToolsResponse;
  }

  async GooseResourceRead(
    params: ReadResourceRequest,
  ): Promise<ReadResourceResponse> {
    const raw = await this.conn.extMethod("_goose/resource/read", params);
    return zReadResourceResponse.parse(raw) as ReadResourceResponse;
  }

  async GooseWorkingDirUpdate(params: UpdateWorkingDirRequest): Promise<void> {
    await this.conn.extMethod("_goose/working_dir/update", params);
  }

  async sessionDelete(params: DeleteSessionRequest): Promise<void> {
    await this.conn.extMethod("session/delete", params);
  }

  async GooseConfigExtensions(
    params: GetExtensionsRequest,
  ): Promise<GetExtensionsResponse> {
    const raw = await this.conn.extMethod("_goose/config/extensions", params);
    return zGetExtensionsResponse.parse(raw) as GetExtensionsResponse;
  }

  async GooseSessionProviderUpdate(
    params: UpdateProviderRequest,
  ): Promise<UpdateProviderResponse> {
    const raw = await this.conn.extMethod(
      "_goose/session/provider/update",
      params,
    );
    return zUpdateProviderResponse.parse(raw) as UpdateProviderResponse;
  }

  async GooseProvidersList(
    params: ListProvidersRequest,
  ): Promise<ListProvidersResponse> {
    const raw = await this.conn.extMethod("_goose/providers/list", params);
    return zListProvidersResponse.parse(raw) as ListProvidersResponse;
  }

  async GooseConfigRead(
    params: ReadConfigRequest,
  ): Promise<ReadConfigResponse> {
    const raw = await this.conn.extMethod("_goose/config/read", params);
    return zReadConfigResponse.parse(raw) as ReadConfigResponse;
  }

  async GooseConfigUpsert(params: UpsertConfigRequest): Promise<void> {
    await this.conn.extMethod("_goose/config/upsert", params);
  }

  async GooseConfigRemove(params: RemoveConfigRequest): Promise<void> {
    await this.conn.extMethod("_goose/config/remove", params);
  }

  async GooseSecretCheck(
    params: CheckSecretRequest,
  ): Promise<CheckSecretResponse> {
    const raw = await this.conn.extMethod("_goose/secret/check", params);
    return zCheckSecretResponse.parse(raw) as CheckSecretResponse;
  }

  async GooseSecretUpsert(params: UpsertSecretRequest): Promise<void> {
    await this.conn.extMethod("_goose/secret/upsert", params);
  }

  async GooseSecretRemove(params: RemoveSecretRequest): Promise<void> {
    await this.conn.extMethod("_goose/secret/remove", params);
  }

  async GooseSessionExport(
    params: ExportSessionRequest,
  ): Promise<ExportSessionResponse> {
    const raw = await this.conn.extMethod("_goose/session/export", params);
    return zExportSessionResponse.parse(raw) as ExportSessionResponse;
  }

  async GooseSessionImport(
    params: ImportSessionRequest,
  ): Promise<ImportSessionResponse> {
    const raw = await this.conn.extMethod("_goose/session/import", params);
    return zImportSessionResponse.parse(raw) as ImportSessionResponse;
  }

  async GooseSessionArchive(params: ArchiveSessionRequest): Promise<void> {
    await this.conn.extMethod("_goose/session/archive", params);
  }

  async GooseSessionUnarchive(params: UnarchiveSessionRequest): Promise<void> {
    await this.conn.extMethod("_goose/session/unarchive", params);
  }
}
