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
  CreateSourceRequest,
  CreateSourceResponse,
  CustomProviderCreateRequest,
  CustomProviderCreateResponse,
  CustomProviderDeleteRequest,
  CustomProviderDeleteResponse,
  CustomProviderReadRequest,
  CustomProviderReadResponse,
  CustomProviderUpdateRequest,
  CustomProviderUpdateResponse,
  DefaultsReadRequest,
  DefaultsReadResponse,
  DefaultsSaveRequest,
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
  DictationSecretDeleteRequest,
  DictationSecretSaveRequest,
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
  GooseToolCallRequest,
  GooseToolCallResponse,
  ImportSessionRequest,
  ImportSessionResponse,
  ImportSourcesRequest,
  ImportSourcesResponse,
  ListProvidersRequest,
  ListProvidersResponse,
  ListSourcesRequest,
  ListSourcesResponse,
  OnboardingImportApplyRequest,
  OnboardingImportApplyResponse,
  OnboardingImportScanRequest,
  OnboardingImportScanResponse,
  PreferencesReadRequest,
  PreferencesReadResponse,
  PreferencesRemoveRequest,
  PreferencesSaveRequest,
  ProviderCatalogListRequest,
  ProviderCatalogListResponse,
  ProviderCatalogTemplateRequest,
  ProviderCatalogTemplateResponse,
  ProviderConfigAuthenticateRequest,
  ProviderConfigChangeResponse,
  ProviderConfigDeleteRequest,
  ProviderConfigReadRequest,
  ProviderConfigReadResponse,
  ProviderConfigSaveRequest,
  ProviderConfigStatusRequest,
  ProviderConfigStatusResponse,
  ProviderSetupCatalogListRequest,
  ProviderSetupCatalogListResponse,
  ReadResourceRequest,
  ReadResourceResponse,
  RefreshProviderInventoryRequest,
  RefreshProviderInventoryResponse,
  RemoveConfigExtensionRequest,
  RemoveExtensionRequest,
  RenameSessionRequest,
  ToggleConfigExtensionRequest,
  UnarchiveSessionRequest,
  UpdateSessionProjectRequest,
  UpdateSourceRequest,
  UpdateSourceResponse,
  UpdateWorkingDirRequest,
} from './types.gen.js';
import {
  zCreateSourceResponse,
  zCustomProviderCreateResponse,
  zCustomProviderDeleteResponse,
  zCustomProviderReadResponse,
  zCustomProviderUpdateResponse,
  zDefaultsReadResponse,
  zDictationConfigResponse,
  zDictationModelDownloadProgressResponse,
  zDictationModelsListResponse,
  zDictationTranscribeResponse,
  zExportSessionResponse,
  zExportSourceResponse,
  zGetExtensionsResponse,
  zGetSessionExtensionsResponse,
  zGetToolsResponse,
  zGooseToolCallResponse,
  zImportSessionResponse,
  zImportSourcesResponse,
  zListProvidersResponse,
  zListSourcesResponse,
  zOnboardingImportApplyResponse,
  zOnboardingImportScanResponse,
  zPreferencesReadResponse,
  zProviderCatalogListResponse,
  zProviderCatalogTemplateResponse,
  zProviderConfigChangeResponse,
  zProviderConfigReadResponse,
  zProviderConfigStatusResponse,
  zProviderSetupCatalogListResponse,
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

  async GooseToolCall(
    params: GooseToolCallRequest,
  ): Promise<GooseToolCallResponse> {
    const raw = await this.conn.extMethod("_goose/tool/call", params);
    return zGooseToolCallResponse.parse(raw) as GooseToolCallResponse;
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

  async GooseProvidersCatalogList(
    params: ProviderCatalogListRequest,
  ): Promise<ProviderCatalogListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/catalog/list",
      params,
    );
    return zProviderCatalogListResponse.parse(
      raw,
    ) as ProviderCatalogListResponse;
  }

  async GooseProvidersSetupCatalogList(
    params: ProviderSetupCatalogListRequest,
  ): Promise<ProviderSetupCatalogListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/setup/catalog/list",
      params,
    );
    return zProviderSetupCatalogListResponse.parse(
      raw,
    ) as ProviderSetupCatalogListResponse;
  }

  async GooseProvidersCatalogTemplate(
    params: ProviderCatalogTemplateRequest,
  ): Promise<ProviderCatalogTemplateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/catalog/template",
      params,
    );
    return zProviderCatalogTemplateResponse.parse(
      raw,
    ) as ProviderCatalogTemplateResponse;
  }

  async GooseProvidersCustomCreate(
    params: CustomProviderCreateRequest,
  ): Promise<CustomProviderCreateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/custom/create",
      params,
    );
    return zCustomProviderCreateResponse.parse(
      raw,
    ) as CustomProviderCreateResponse;
  }

  async GooseProvidersCustomRead(
    params: CustomProviderReadRequest,
  ): Promise<CustomProviderReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/custom/read",
      params,
    );
    return zCustomProviderReadResponse.parse(raw) as CustomProviderReadResponse;
  }

  async GooseProvidersCustomUpdate(
    params: CustomProviderUpdateRequest,
  ): Promise<CustomProviderUpdateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/custom/update",
      params,
    );
    return zCustomProviderUpdateResponse.parse(
      raw,
    ) as CustomProviderUpdateResponse;
  }

  async GooseProvidersCustomDelete(
    params: CustomProviderDeleteRequest,
  ): Promise<CustomProviderDeleteResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/custom/delete",
      params,
    );
    return zCustomProviderDeleteResponse.parse(
      raw,
    ) as CustomProviderDeleteResponse;
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

  async GooseProvidersConfigRead(
    params: ProviderConfigReadRequest,
  ): Promise<ProviderConfigReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/config/read",
      params,
    );
    return zProviderConfigReadResponse.parse(raw) as ProviderConfigReadResponse;
  }

  async GooseProvidersConfigStatus(
    params: ProviderConfigStatusRequest,
  ): Promise<ProviderConfigStatusResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/config/status",
      params,
    );
    return zProviderConfigStatusResponse.parse(
      raw,
    ) as ProviderConfigStatusResponse;
  }

  async GooseProvidersConfigSave(
    params: ProviderConfigSaveRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/config/save",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GooseProvidersConfigDelete(
    params: ProviderConfigDeleteRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/config/delete",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GooseProvidersConfigAuthenticate(
    params: ProviderConfigAuthenticateRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/providers/config/authenticate",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GoosePreferencesRead(
    params: PreferencesReadRequest,
  ): Promise<PreferencesReadResponse> {
    const raw = await this.conn.extMethod("_goose/preferences/read", params);
    return zPreferencesReadResponse.parse(raw) as PreferencesReadResponse;
  }

  async GoosePreferencesSave(params: PreferencesSaveRequest): Promise<void> {
    await this.conn.extMethod("_goose/preferences/save", params);
  }

  async GoosePreferencesRemove(
    params: PreferencesRemoveRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/preferences/remove", params);
  }

  async GooseDefaultsRead(
    params: DefaultsReadRequest,
  ): Promise<DefaultsReadResponse> {
    const raw = await this.conn.extMethod("_goose/defaults/read", params);
    return zDefaultsReadResponse.parse(raw) as DefaultsReadResponse;
  }

  async GooseDefaultsSave(
    params: DefaultsSaveRequest,
  ): Promise<DefaultsReadResponse> {
    const raw = await this.conn.extMethod("_goose/defaults/save", params);
    return zDefaultsReadResponse.parse(raw) as DefaultsReadResponse;
  }

  async GooseOnboardingImportScan(
    params: OnboardingImportScanRequest,
  ): Promise<OnboardingImportScanResponse> {
    const raw = await this.conn.extMethod(
      "_goose/onboarding/import/scan",
      params,
    );
    return zOnboardingImportScanResponse.parse(
      raw,
    ) as OnboardingImportScanResponse;
  }

  async GooseOnboardingImportApply(
    params: OnboardingImportApplyRequest,
  ): Promise<OnboardingImportApplyResponse> {
    const raw = await this.conn.extMethod(
      "_goose/onboarding/import/apply",
      params,
    );
    return zOnboardingImportApplyResponse.parse(
      raw,
    ) as OnboardingImportApplyResponse;
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

  async GooseSessionRename(params: RenameSessionRequest): Promise<void> {
    await this.conn.extMethod("_goose/session/rename", params);
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

  async GooseDictationSecretSave(
    params: DictationSecretSaveRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/secret/save", params);
  }

  async GooseDictationSecretDelete(
    params: DictationSecretDeleteRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/dictation/secret/delete", params);
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
