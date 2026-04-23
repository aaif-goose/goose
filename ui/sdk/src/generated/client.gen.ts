// This file is auto-generated — do not edit manually.

export interface ExtMethodProvider {
  extMethod(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
}

import type {
  AddConfigExtensionRequest,
  AddExtensionRequest,
  ArchiveSessionRequest,
  CheckSecretRequest,
  CheckSecretResponse,
  CreateSourceRequest,
  CreateSourceResponse,
  DeleteSessionRequest,
  DeleteSourceRequest,
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
  ExportSourceRequest,
  ExportSourceResponse,
  GetExtensionsRequest,
  GetExtensionsResponse,
  GetSessionExtensionsRequest,
  GetSessionExtensionsResponse,
  GetToolsRequest,
  GetToolsResponse,
  ImportSessionRequest,
  ImportSessionResponse,
  ImportSourcesRequest,
  ImportSourcesResponse,
  ListProvidersRequest,
  ListProvidersResponse,
  ListSourcesRequest,
  ListSourcesResponse,
  ReadConfigRequest,
  ReadConfigResponse,
  ReadResourceRequest,
  ReadResourceResponse,
  RefreshProviderInventoryRequest,
  RefreshProviderInventoryResponse,
  RemoveConfigExtensionRequest,
  RemoveConfigRequest,
  RemoveExtensionRequest,
  RemoveSecretRequest,
  ToggleConfigExtensionRequest,
  UnarchiveSessionRequest,
  UpdateSessionProjectRequest,
  UpdateSourceRequest,
  UpdateSourceResponse,
  UpdateWorkingDirRequest,
  UpsertConfigRequest,
  UpsertSecretRequest,
} from './types.gen.js';
import {
  zCheckSecretResponse,
  zCreateSourceResponse,
  zDictationConfigResponse,
  zDictationModelDownloadProgressResponse,
  zDictationModelsListResponse,
  zDictationTranscribeResponse,
  zExportSessionResponse,
  zExportSourceResponse,
  zGetExtensionsResponse,
  zGetSessionExtensionsResponse,
  zGetToolsResponse,
  zImportSessionResponse,
  zImportSourcesResponse,
  zListProvidersResponse,
  zListSourcesResponse,
  zReadConfigResponse,
  zReadResourceResponse,
  zRefreshProviderInventoryResponse,
  zUpdateSourceResponse,
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

  async GooseConfigExtensionsAdd(
    params: AddConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/config/extensions/add", params);
  }

  async GooseConfigExtensionsRemove(
    params: RemoveConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/config/extensions/remove", params);
  }

  async GooseConfigExtensionsToggle(
    params: ToggleConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/config/extensions/toggle", params);
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

  async GooseSessionUpdateProject(
    params: UpdateSessionProjectRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/session/update_project", params);
  }

  async GooseSessionArchive(params: ArchiveSessionRequest): Promise<void> {
    await this.conn.extMethod("_goose/session/archive", params);
  }

  async GooseSessionUnarchive(params: UnarchiveSessionRequest): Promise<void> {
    await this.conn.extMethod("_goose/session/unarchive", params);
  }

  async GooseSourcesCreate(
    params: CreateSourceRequest,
  ): Promise<CreateSourceResponse> {
    const raw = await this.conn.extMethod("_goose/sources/create", params);
    return zCreateSourceResponse.parse(raw) as CreateSourceResponse;
  }

  async GooseSourcesList(
    params: ListSourcesRequest,
  ): Promise<ListSourcesResponse> {
    const raw = await this.conn.extMethod("_goose/sources/list", params);
    return zListSourcesResponse.parse(raw) as ListSourcesResponse;
  }

  async GooseSourcesUpdate(
    params: UpdateSourceRequest,
  ): Promise<UpdateSourceResponse> {
    const raw = await this.conn.extMethod("_goose/sources/update", params);
    return zUpdateSourceResponse.parse(raw) as UpdateSourceResponse;
  }

  async GooseSourcesDelete(params: DeleteSourceRequest): Promise<void> {
    await this.conn.extMethod("_goose/sources/delete", params);
  }

  async GooseSourcesExport(
    params: ExportSourceRequest,
  ): Promise<ExportSourceResponse> {
    const raw = await this.conn.extMethod("_goose/sources/export", params);
    return zExportSourceResponse.parse(raw) as ExportSourceResponse;
  }

  async GooseSourcesImport(
    params: ImportSourcesRequest,
  ): Promise<ImportSourcesResponse> {
    const raw = await this.conn.extMethod("_goose/sources/import", params);
    return zImportSourcesResponse.parse(raw) as ImportSourcesResponse;
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
