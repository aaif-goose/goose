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
  DictationConfigRequest,
  DictationConfigResponse,
  DictationModelCancelRequest,
  DictationModelDeleteRequest,
  DictationModelDownloadProgressRequest,
  DictationModelDownloadProgressResponse,
  DictationModelDownloadRequest,
  DictationModelSelectRequest,
  DictationModelsListRequest,
  DictationModelsListResponse,
  DictationTranscribeRequest,
  DictationTranscribeResponse,
  ExportSessionRequest,
  ExportSessionResponse,
  GetExtensionsRequest,
  GetExtensionsResponse,
  GetProviderDetailsRequest,
  GetProviderDetailsResponse,
  GetProviderInventoryRequest,
  GetProviderInventoryResponse,
  GetSessionExtensionsRequest,
  GetSessionExtensionsResponse,
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
  RefreshProviderInventoryRequest,
  RefreshProviderInventoryResponse,
  RemoveConfigRequest,
  RemoveExtensionRequest,
  RemoveSecretRequest,
  UnarchiveSessionRequest,
  UpdateWorkingDirRequest,
  UpsertConfigRequest,
  UpsertSecretRequest,
} from './types.gen.js';
import {
  zCheckSecretResponse,
  zDictationConfigResponse,
  zDictationModelDownloadProgressResponse,
  zDictationModelsListResponse,
  zDictationTranscribeResponse,
  zExportSessionResponse,
  zGetExtensionsResponse,
  zGetProviderDetailsResponse,
  zGetProviderInventoryResponse,
  zGetSessionExtensionsResponse,
  zGetToolsResponse,
  zImportSessionResponse,
  zListProvidersResponse,
  zReadConfigResponse,
  zReadResourceResponse,
  zRefreshProviderInventoryResponse,
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

  async GooseSessionExtensions(
    params: GetSessionExtensionsRequest,
  ): Promise<GetSessionExtensionsResponse> {
    const raw = await this.conn.extMethod("_goose/session/extensions", params);
    return zGetSessionExtensionsResponse.parse(
      raw,
    ) as GetSessionExtensionsResponse;
  }

  async GooseProvidersList(
    params: ListProvidersRequest,
  ): Promise<ListProvidersResponse> {
    const raw = await this.conn.extMethod("_goose/providers/list", params);
    return zListProvidersResponse.parse(raw) as ListProvidersResponse;
  }

  async GooseProvidersDetails(
    params: GetProviderDetailsRequest,
  ): Promise<GetProviderDetailsResponse> {
    const raw = await this.conn.extMethod("_goose/providers/details", params);
    return zGetProviderDetailsResponse.parse(raw) as GetProviderDetailsResponse;
  }

  async GooseProvidersInventory(
    params: GetProviderInventoryRequest,
  ): Promise<GetProviderInventoryResponse> {
    const raw = await this.conn.extMethod("_goose/providers/inventory", params);
    return zGetProviderInventoryResponse.parse(
      raw,
    ) as GetProviderInventoryResponse;
  }

  async GooseProvidersInventoryRefresh(
    params: RefreshProviderInventoryRequest,
  ): Promise<RefreshProviderInventoryResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/inventory/refresh",
      params,
    );
    return zRefreshProviderInventoryResponse.parse(
      raw,
    ) as RefreshProviderInventoryResponse;
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

  async GooseDictationTranscribe(
    params: DictationTranscribeRequest,
  ): Promise<DictationTranscribeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/dictation/transcribe",
      params,
    );
    return zDictationTranscribeResponse.parse(
      raw,
    ) as DictationTranscribeResponse;
  }

  async GooseDictationConfig(
    params: DictationConfigRequest,
  ): Promise<DictationConfigResponse> {
    const raw = await this.conn.extMethod("_goose/dictation/config", params);
    return zDictationConfigResponse.parse(raw) as DictationConfigResponse;
  }

  async GooseDictationModelsList(
    params: DictationModelsListRequest,
  ): Promise<DictationModelsListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/dictation/models/list",
      params,
    );
    return zDictationModelsListResponse.parse(
      raw,
    ) as DictationModelsListResponse;
  }

  async GooseDictationModelsDownload(
    params: DictationModelDownloadRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/models/download", params);
  }

  async GooseDictationModelsDownloadProgress(
    params: DictationModelDownloadProgressRequest,
  ): Promise<DictationModelDownloadProgressResponse> {
    const raw = await this.conn.extMethod(
      "_goose/dictation/models/download/progress",
      params,
    );
    return zDictationModelDownloadProgressResponse.parse(
      raw,
    ) as DictationModelDownloadProgressResponse;
  }

  async GooseDictationModelsCancel(
    params: DictationModelCancelRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/models/cancel", params);
  }

  async GooseDictationModelsDelete(
    params: DictationModelDeleteRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/models/delete", params);
  }

  async GooseDictationModelSelect(
    params: DictationModelSelectRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/model/select", params);
  }
}
