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

  async GooseUnstableSessionExtensionsAdd(
    params: AddExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/extensions/add", params);
  }

  async GooseUnstableSessionExtensionsRemove(
    params: RemoveExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/session/extensions/remove",
      params,
    );
  }

  async GooseUnstableToolsList(
    params: GetToolsRequest,
  ): Promise<GetToolsResponse> {
    const raw = await this.conn.extMethod("_goose/unstable/tools/list", params);
    return zGetToolsResponse.parse(raw) as GetToolsResponse;
  }

  async GooseUnstableToolsCall(
    params: GooseToolCallRequest,
  ): Promise<GooseToolCallResponse> {
    const raw = await this.conn.extMethod("_goose/unstable/tools/call", params);
    return zGooseToolCallResponse.parse(raw) as GooseToolCallResponse;
  }

  async GooseUnstableResourcesRead(
    params: ReadResourceRequest,
  ): Promise<ReadResourceResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/resources/read",
      params,
    );
    return zReadResourceResponse.parse(raw) as ReadResourceResponse;
  }

  async GooseUnstableSessionWorkingDirUpdate(
    params: UpdateWorkingDirRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/session/working-dir/update",
      params,
    );
  }

  async sessionDelete(params: DeleteSessionRequest): Promise<void> {
    await this.conn.extMethod("session/delete", params);
  }

  async GooseUnstableConfigExtensionsList(
    params: GetExtensionsRequest,
  ): Promise<GetExtensionsResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/config/extensions/list",
      params,
    );
    return zGetExtensionsResponse.parse(raw) as GetExtensionsResponse;
  }

  async GooseUnstableConfigExtensionsAdd(
    params: AddConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/config/extensions/add", params);
  }

  async GooseUnstableConfigExtensionsRemove(
    params: RemoveConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/config/extensions/remove",
      params,
    );
  }

  async GooseUnstableConfigExtensionsToggle(
    params: ToggleConfigExtensionRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/config/extensions/toggle",
      params,
    );
  }

  async GooseUnstableSessionExtensionsList(
    params: GetSessionExtensionsRequest,
  ): Promise<GetSessionExtensionsResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/extensions/list",
      params,
    );
    return zGetSessionExtensionsResponse.parse(
      raw,
    ) as GetSessionExtensionsResponse;
  }

  async GooseUnstableProvidersList(
    params: ListProvidersRequest,
  ): Promise<ListProvidersResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/list",
      params,
    );
    return zListProvidersResponse.parse(raw) as ListProvidersResponse;
  }

  async GooseUnstableProvidersCatalogList(
    params: ProviderCatalogListRequest,
  ): Promise<ProviderCatalogListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/catalog/list",
      params,
    );
    return zProviderCatalogListResponse.parse(
      raw,
    ) as ProviderCatalogListResponse;
  }

  async GooseUnstableProvidersSetupCatalogList(
    params: ProviderSetupCatalogListRequest,
  ): Promise<ProviderSetupCatalogListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/setup/catalog/list",
      params,
    );
    return zProviderSetupCatalogListResponse.parse(
      raw,
    ) as ProviderSetupCatalogListResponse;
  }

  async GooseUnstableProvidersCatalogTemplate(
    params: ProviderCatalogTemplateRequest,
  ): Promise<ProviderCatalogTemplateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/catalog/template",
      params,
    );
    return zProviderCatalogTemplateResponse.parse(
      raw,
    ) as ProviderCatalogTemplateResponse;
  }

  async GooseUnstableProvidersCustomCreate(
    params: CustomProviderCreateRequest,
  ): Promise<CustomProviderCreateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/create",
      params,
    );
    return zCustomProviderCreateResponse.parse(
      raw,
    ) as CustomProviderCreateResponse;
  }

  async GooseUnstableProvidersCustomRead(
    params: CustomProviderReadRequest,
  ): Promise<CustomProviderReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/read",
      params,
    );
    return zCustomProviderReadResponse.parse(raw) as CustomProviderReadResponse;
  }

  async GooseUnstableProvidersCustomUpdate(
    params: CustomProviderUpdateRequest,
  ): Promise<CustomProviderUpdateResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/update",
      params,
    );
    return zCustomProviderUpdateResponse.parse(
      raw,
    ) as CustomProviderUpdateResponse;
  }

  async GooseUnstableProvidersCustomDelete(
    params: CustomProviderDeleteRequest,
  ): Promise<CustomProviderDeleteResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/delete",
      params,
    );
    return zCustomProviderDeleteResponse.parse(
      raw,
    ) as CustomProviderDeleteResponse;
  }

  async GooseUnstableProvidersInventoryRefresh(
    params: RefreshProviderInventoryRequest,
  ): Promise<RefreshProviderInventoryResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/inventory/refresh",
      params,
    );
    return zRefreshProviderInventoryResponse.parse(
      raw,
    ) as RefreshProviderInventoryResponse;
  }

  async GooseUnstableProvidersConfigRead(
    params: ProviderConfigReadRequest,
  ): Promise<ProviderConfigReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/read",
      params,
    );
    return zProviderConfigReadResponse.parse(raw) as ProviderConfigReadResponse;
  }

  async GooseUnstableProvidersConfigStatus(
    params: ProviderConfigStatusRequest,
  ): Promise<ProviderConfigStatusResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/status",
      params,
    );
    return zProviderConfigStatusResponse.parse(
      raw,
    ) as ProviderConfigStatusResponse;
  }

  async GooseUnstableProvidersConfigSave(
    params: ProviderConfigSaveRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/save",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GooseUnstableProvidersConfigDelete(
    params: ProviderConfigDeleteRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/delete",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GooseUnstableProvidersConfigAuthenticate(
    params: ProviderConfigAuthenticateRequest,
  ): Promise<ProviderConfigChangeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/authenticate",
      params,
    );
    return zProviderConfigChangeResponse.parse(
      raw,
    ) as ProviderConfigChangeResponse;
  }

  async GooseUnstablePreferencesRead(
    params: PreferencesReadRequest,
  ): Promise<PreferencesReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/preferences/read",
      params,
    );
    return zPreferencesReadResponse.parse(raw) as PreferencesReadResponse;
  }

  async GooseUnstablePreferencesSave(
    params: PreferencesSaveRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/preferences/save", params);
  }

  async GooseUnstablePreferencesRemove(
    params: PreferencesRemoveRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/preferences/remove", params);
  }

  async GooseUnstableDefaultsRead(
    params: DefaultsReadRequest,
  ): Promise<DefaultsReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/defaults/read",
      params,
    );
    return zDefaultsReadResponse.parse(raw) as DefaultsReadResponse;
  }

  async GooseUnstableDefaultsSave(
    params: DefaultsSaveRequest,
  ): Promise<DefaultsReadResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/defaults/save",
      params,
    );
    return zDefaultsReadResponse.parse(raw) as DefaultsReadResponse;
  }

  async GooseUnstableOnboardingImportScan(
    params: OnboardingImportScanRequest,
  ): Promise<OnboardingImportScanResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/onboarding/import/scan",
      params,
    );
    return zOnboardingImportScanResponse.parse(
      raw,
    ) as OnboardingImportScanResponse;
  }

  async GooseUnstableOnboardingImportApply(
    params: OnboardingImportApplyRequest,
  ): Promise<OnboardingImportApplyResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/onboarding/import/apply",
      params,
    );
    return zOnboardingImportApplyResponse.parse(
      raw,
    ) as OnboardingImportApplyResponse;
  }

  async GooseUnstableSessionExport(
    params: ExportSessionRequest,
  ): Promise<ExportSessionResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/export",
      params,
    );
    return zExportSessionResponse.parse(raw) as ExportSessionResponse;
  }

  async GooseUnstableSessionImport(
    params: ImportSessionRequest,
  ): Promise<ImportSessionResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/import",
      params,
    );
    return zImportSessionResponse.parse(raw) as ImportSessionResponse;
  }

  async GooseUnstableSessionProjectUpdate(
    params: UpdateSessionProjectRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/project/update", params);
  }

  async GooseUnstableSessionRename(
    params: RenameSessionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/rename", params);
  }

  async GooseUnstableSessionArchive(
    params: ArchiveSessionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/archive", params);
  }

  async GooseUnstableSessionUnarchive(
    params: UnarchiveSessionRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/unarchive", params);
  }

  async GooseUnstableSourcesCreate(
    params: CreateSourceRequest,
  ): Promise<CreateSourceResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/create",
      params,
    );
    return zCreateSourceResponse.parse(raw) as CreateSourceResponse;
  }

  async GooseUnstableSourcesList(
    params: ListSourcesRequest,
  ): Promise<ListSourcesResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/list",
      params,
    );
    return zListSourcesResponse.parse(raw) as ListSourcesResponse;
  }

  async GooseUnstableSourcesUpdate(
    params: UpdateSourceRequest,
  ): Promise<UpdateSourceResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/update",
      params,
    );
    return zUpdateSourceResponse.parse(raw) as UpdateSourceResponse;
  }

  async GooseUnstableSourcesDelete(params: DeleteSourceRequest): Promise<void> {
    await this.conn.extMethod("_goose/unstable/sources/delete", params);
  }

  async GooseUnstableSourcesExport(
    params: ExportSourceRequest,
  ): Promise<ExportSourceResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/export",
      params,
    );
    return zExportSourceResponse.parse(raw) as ExportSourceResponse;
  }

  async GooseUnstableSourcesImport(
    params: ImportSourcesRequest,
  ): Promise<ImportSourcesResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/import",
      params,
    );
    return zImportSourcesResponse.parse(raw) as ImportSourcesResponse;
  }

  async GooseUnstableDictationTranscribe(
    params: DictationTranscribeRequest,
  ): Promise<DictationTranscribeResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/transcribe",
      params,
    );
    return zDictationTranscribeResponse.parse(
      raw,
    ) as DictationTranscribeResponse;
  }

  async GooseUnstableDictationConfig(
    params: DictationConfigRequest,
  ): Promise<DictationConfigResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/config",
      params,
    );
    return zDictationConfigResponse.parse(raw) as DictationConfigResponse;
  }

  async GooseUnstableDictationSecretSave(
    params: DictationSecretSaveRequest,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/dictation/secret/save", params);
  }

  async GooseUnstableDictationSecretDelete(
    params: DictationSecretDeleteRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/secret/delete",
      params,
    );
  }

  async GooseUnstableDictationModelsList(
    params: DictationModelsListRequest,
  ): Promise<DictationModelsListResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/models/list",
      params,
    );
    return zDictationModelsListResponse.parse(
      raw,
    ) as DictationModelsListResponse;
  }

  async GooseUnstableDictationModelsDownload(
    params: DictationModelDownloadRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/download",
      params,
    );
  }

  async GooseUnstableDictationModelsDownloadProgress(
    params: DictationModelDownloadProgressRequest,
  ): Promise<DictationModelDownloadProgressResponse> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/models/download/progress",
      params,
    );
    return zDictationModelDownloadProgressResponse.parse(
      raw,
    ) as DictationModelDownloadProgressResponse;
  }

  async GooseUnstableDictationModelsCancel(
    params: DictationModelCancelRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/cancel",
      params,
    );
  }

  async GooseUnstableDictationModelsDelete(
    params: DictationModelDeleteRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/delete",
      params,
    );
  }

  async GooseUnstableDictationModelsSelect(
    params: DictationModelSelectRequest,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/select",
      params,
    );
  }
}
