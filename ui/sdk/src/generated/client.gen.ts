// This file is auto-generated — do not edit manually.

export interface ExtMethodProvider {
  extMethod(
    method: string,
    params: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
}

import type {
  AddConfigExtensionRequest_unstable,
  AddExtensionRequest_unstable,
  ArchiveSessionRequest_unstable,
  CreateSourceRequest_unstable,
  CreateSourceResponse_unstable,
  CustomProviderCreateRequest_unstable,
  CustomProviderCreateResponse_unstable,
  CustomProviderDeleteRequest_unstable,
  CustomProviderDeleteResponse_unstable,
  CustomProviderReadRequest_unstable,
  CustomProviderReadResponse_unstable,
  CustomProviderUpdateRequest_unstable,
  CustomProviderUpdateResponse_unstable,
  DefaultsReadRequest_unstable,
  DefaultsReadResponse_unstable,
  DefaultsSaveRequest_unstable,
  DeleteSessionRequest,
  DeleteSourceRequest_unstable,
  DictationConfigRequest_unstable,
  DictationConfigResponse_unstable,
  DictationModelCancelRequest_unstable,
  DictationModelDeleteRequest_unstable,
  DictationModelDownloadProgressRequest_unstable,
  DictationModelDownloadProgressResponse_unstable,
  DictationModelDownloadRequest_unstable,
  DictationModelSelectRequest_unstable,
  DictationModelsListRequest_unstable,
  DictationModelsListResponse_unstable,
  DictationSecretDeleteRequest_unstable,
  DictationSecretSaveRequest_unstable,
  DictationTranscribeRequest_unstable,
  DictationTranscribeResponse_unstable,
  ExportSessionRequest_unstable,
  ExportSessionResponse_unstable,
  ExportSourceRequest_unstable,
  ExportSourceResponse_unstable,
  GetExtensionsRequest_unstable,
  GetExtensionsResponse_unstable,
  GetSessionExtensionsRequest_unstable,
  GetSessionExtensionsResponse_unstable,
  GetToolsRequest_unstable,
  GetToolsResponse_unstable,
  GooseToolCallRequest_unstable,
  GooseToolCallResponse_unstable,
  ImportSessionRequest_unstable,
  ImportSessionResponse_unstable,
  ImportSourcesRequest_unstable,
  ImportSourcesResponse_unstable,
  ListProvidersRequest_unstable,
  ListProvidersResponse_unstable,
  ListSourcesRequest_unstable,
  ListSourcesResponse_unstable,
  OnboardingImportApplyRequest_unstable,
  OnboardingImportApplyResponse_unstable,
  OnboardingImportScanRequest_unstable,
  OnboardingImportScanResponse_unstable,
  PreferencesReadRequest_unstable,
  PreferencesReadResponse_unstable,
  PreferencesRemoveRequest_unstable,
  PreferencesSaveRequest_unstable,
  ProviderCatalogListRequest_unstable,
  ProviderCatalogListResponse_unstable,
  ProviderCatalogTemplateRequest_unstable,
  ProviderCatalogTemplateResponse_unstable,
  ProviderConfigAuthenticateRequest_unstable,
  ProviderConfigChangeResponse_unstable,
  ProviderConfigDeleteRequest_unstable,
  ProviderConfigReadRequest_unstable,
  ProviderConfigReadResponse_unstable,
  ProviderConfigSaveRequest_unstable,
  ProviderConfigStatusRequest_unstable,
  ProviderConfigStatusResponse_unstable,
  ProviderSetupCatalogListRequest_unstable,
  ProviderSetupCatalogListResponse_unstable,
  ReadResourceRequest_unstable,
  ReadResourceResponse_unstable,
  RefreshProviderInventoryRequest_unstable,
  RefreshProviderInventoryResponse_unstable,
  RemoveConfigExtensionRequest_unstable,
  RemoveExtensionRequest_unstable,
  RenameSessionRequest_unstable,
  ToggleConfigExtensionRequest_unstable,
  UnarchiveSessionRequest_unstable,
  UpdateSessionProjectRequest_unstable,
  UpdateSourceRequest_unstable,
  UpdateSourceResponse_unstable,
  UpdateWorkingDirRequest_unstable,
} from './types.gen.js';
import {
  zCreateSourceResponse_unstable,
  zCustomProviderCreateResponse_unstable,
  zCustomProviderDeleteResponse_unstable,
  zCustomProviderReadResponse_unstable,
  zCustomProviderUpdateResponse_unstable,
  zDefaultsReadResponse_unstable,
  zDictationConfigResponse_unstable,
  zDictationModelDownloadProgressResponse_unstable,
  zDictationModelsListResponse_unstable,
  zDictationTranscribeResponse_unstable,
  zExportSessionResponse_unstable,
  zExportSourceResponse_unstable,
  zGetExtensionsResponse_unstable,
  zGetSessionExtensionsResponse_unstable,
  zGetToolsResponse_unstable,
  zGooseToolCallResponse_unstable,
  zImportSessionResponse_unstable,
  zImportSourcesResponse_unstable,
  zListProvidersResponse_unstable,
  zListSourcesResponse_unstable,
  zOnboardingImportApplyResponse_unstable,
  zOnboardingImportScanResponse_unstable,
  zPreferencesReadResponse_unstable,
  zProviderCatalogListResponse_unstable,
  zProviderCatalogTemplateResponse_unstable,
  zProviderConfigChangeResponse_unstable,
  zProviderConfigReadResponse_unstable,
  zProviderConfigStatusResponse_unstable,
  zProviderSetupCatalogListResponse_unstable,
  zReadResourceResponse_unstable,
  zRefreshProviderInventoryResponse_unstable,
  zUpdateSourceResponse_unstable,
} from './zod.gen.js';

export class GooseExtClient {
  constructor(private conn: ExtMethodProvider) {}

  async sessionExtensionsAdd_unstable(
    params: AddExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/extensions/add", params);
  }

  async sessionExtensionsRemove_unstable(
    params: RemoveExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/session/extensions/remove",
      params,
    );
  }

  async toolsList_unstable(
    params: GetToolsRequest_unstable,
  ): Promise<GetToolsResponse_unstable> {
    const raw = await this.conn.extMethod("_goose/unstable/tools/list", params);
    return zGetToolsResponse_unstable.parse(raw) as GetToolsResponse_unstable;
  }

  async toolsCall_unstable(
    params: GooseToolCallRequest_unstable,
  ): Promise<GooseToolCallResponse_unstable> {
    const raw = await this.conn.extMethod("_goose/unstable/tools/call", params);
    return zGooseToolCallResponse_unstable.parse(
      raw,
    ) as GooseToolCallResponse_unstable;
  }

  async resourcesRead_unstable(
    params: ReadResourceRequest_unstable,
  ): Promise<ReadResourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/resources/read",
      params,
    );
    return zReadResourceResponse_unstable.parse(
      raw,
    ) as ReadResourceResponse_unstable;
  }

  async sessionWorkingDirUpdate_unstable(
    params: UpdateWorkingDirRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/session/working-dir/update",
      params,
    );
  }

  async sessionDelete(params: DeleteSessionRequest): Promise<void> {
    await this.conn.extMethod("session/delete", params);
  }

  async configExtensionsList_unstable(
    params: GetExtensionsRequest_unstable,
  ): Promise<GetExtensionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/config/extensions/list",
      params,
    );
    return zGetExtensionsResponse_unstable.parse(
      raw,
    ) as GetExtensionsResponse_unstable;
  }

  async configExtensionsAdd_unstable(
    params: AddConfigExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/config/extensions/add", params);
  }

  async configExtensionsRemove_unstable(
    params: RemoveConfigExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/config/extensions/remove",
      params,
    );
  }

  async configExtensionsToggle_unstable(
    params: ToggleConfigExtensionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/config/extensions/toggle",
      params,
    );
  }

  async sessionExtensionsList_unstable(
    params: GetSessionExtensionsRequest_unstable,
  ): Promise<GetSessionExtensionsResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/extensions/list",
      params,
    );
    return zGetSessionExtensionsResponse_unstable.parse(
      raw,
    ) as GetSessionExtensionsResponse_unstable;
  }

  async providersList_unstable(
    params: ListProvidersRequest_unstable,
  ): Promise<ListProvidersResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/list",
      params,
    );
    return zListProvidersResponse_unstable.parse(
      raw,
    ) as ListProvidersResponse_unstable;
  }

  async providersCatalogList_unstable(
    params: ProviderCatalogListRequest_unstable,
  ): Promise<ProviderCatalogListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/catalog/list",
      params,
    );
    return zProviderCatalogListResponse_unstable.parse(
      raw,
    ) as ProviderCatalogListResponse_unstable;
  }

  async providersSetupCatalogList_unstable(
    params: ProviderSetupCatalogListRequest_unstable,
  ): Promise<ProviderSetupCatalogListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/setup/catalog/list",
      params,
    );
    return zProviderSetupCatalogListResponse_unstable.parse(
      raw,
    ) as ProviderSetupCatalogListResponse_unstable;
  }

  async providersCatalogTemplate_unstable(
    params: ProviderCatalogTemplateRequest_unstable,
  ): Promise<ProviderCatalogTemplateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/catalog/template",
      params,
    );
    return zProviderCatalogTemplateResponse_unstable.parse(
      raw,
    ) as ProviderCatalogTemplateResponse_unstable;
  }

  async providersCustomCreate_unstable(
    params: CustomProviderCreateRequest_unstable,
  ): Promise<CustomProviderCreateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/create",
      params,
    );
    return zCustomProviderCreateResponse_unstable.parse(
      raw,
    ) as CustomProviderCreateResponse_unstable;
  }

  async providersCustomRead_unstable(
    params: CustomProviderReadRequest_unstable,
  ): Promise<CustomProviderReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/read",
      params,
    );
    return zCustomProviderReadResponse_unstable.parse(
      raw,
    ) as CustomProviderReadResponse_unstable;
  }

  async providersCustomUpdate_unstable(
    params: CustomProviderUpdateRequest_unstable,
  ): Promise<CustomProviderUpdateResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/update",
      params,
    );
    return zCustomProviderUpdateResponse_unstable.parse(
      raw,
    ) as CustomProviderUpdateResponse_unstable;
  }

  async providersCustomDelete_unstable(
    params: CustomProviderDeleteRequest_unstable,
  ): Promise<CustomProviderDeleteResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/custom/delete",
      params,
    );
    return zCustomProviderDeleteResponse_unstable.parse(
      raw,
    ) as CustomProviderDeleteResponse_unstable;
  }

  async providersInventoryRefresh_unstable(
    params: RefreshProviderInventoryRequest_unstable,
  ): Promise<RefreshProviderInventoryResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/inventory/refresh",
      params,
    );
    return zRefreshProviderInventoryResponse_unstable.parse(
      raw,
    ) as RefreshProviderInventoryResponse_unstable;
  }

  async providersConfigRead_unstable(
    params: ProviderConfigReadRequest_unstable,
  ): Promise<ProviderConfigReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/read",
      params,
    );
    return zProviderConfigReadResponse_unstable.parse(
      raw,
    ) as ProviderConfigReadResponse_unstable;
  }

  async providersConfigStatus_unstable(
    params: ProviderConfigStatusRequest_unstable,
  ): Promise<ProviderConfigStatusResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/status",
      params,
    );
    return zProviderConfigStatusResponse_unstable.parse(
      raw,
    ) as ProviderConfigStatusResponse_unstable;
  }

  async providersConfigSave_unstable(
    params: ProviderConfigSaveRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/save",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async providersConfigDelete_unstable(
    params: ProviderConfigDeleteRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/delete",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async providersConfigAuthenticate_unstable(
    params: ProviderConfigAuthenticateRequest_unstable,
  ): Promise<ProviderConfigChangeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/providers/config/authenticate",
      params,
    );
    return zProviderConfigChangeResponse_unstable.parse(
      raw,
    ) as ProviderConfigChangeResponse_unstable;
  }

  async preferencesRead_unstable(
    params: PreferencesReadRequest_unstable,
  ): Promise<PreferencesReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/preferences/read",
      params,
    );
    return zPreferencesReadResponse_unstable.parse(
      raw,
    ) as PreferencesReadResponse_unstable;
  }

  async preferencesSave_unstable(
    params: PreferencesSaveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/preferences/save", params);
  }

  async preferencesRemove_unstable(
    params: PreferencesRemoveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/preferences/remove", params);
  }

  async defaultsRead_unstable(
    params: DefaultsReadRequest_unstable,
  ): Promise<DefaultsReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/defaults/read",
      params,
    );
    return zDefaultsReadResponse_unstable.parse(
      raw,
    ) as DefaultsReadResponse_unstable;
  }

  async defaultsSave_unstable(
    params: DefaultsSaveRequest_unstable,
  ): Promise<DefaultsReadResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/defaults/save",
      params,
    );
    return zDefaultsReadResponse_unstable.parse(
      raw,
    ) as DefaultsReadResponse_unstable;
  }

  async onboardingImportScan_unstable(
    params: OnboardingImportScanRequest_unstable,
  ): Promise<OnboardingImportScanResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/onboarding/import/scan",
      params,
    );
    return zOnboardingImportScanResponse_unstable.parse(
      raw,
    ) as OnboardingImportScanResponse_unstable;
  }

  async onboardingImportApply_unstable(
    params: OnboardingImportApplyRequest_unstable,
  ): Promise<OnboardingImportApplyResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/onboarding/import/apply",
      params,
    );
    return zOnboardingImportApplyResponse_unstable.parse(
      raw,
    ) as OnboardingImportApplyResponse_unstable;
  }

  async sessionExport_unstable(
    params: ExportSessionRequest_unstable,
  ): Promise<ExportSessionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/export",
      params,
    );
    return zExportSessionResponse_unstable.parse(
      raw,
    ) as ExportSessionResponse_unstable;
  }

  async sessionImport_unstable(
    params: ImportSessionRequest_unstable,
  ): Promise<ImportSessionResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/session/import",
      params,
    );
    return zImportSessionResponse_unstable.parse(
      raw,
    ) as ImportSessionResponse_unstable;
  }

  async sessionProjectUpdate_unstable(
    params: UpdateSessionProjectRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/project/update", params);
  }

  async sessionRename_unstable(
    params: RenameSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/rename", params);
  }

  async sessionArchive_unstable(
    params: ArchiveSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/archive", params);
  }

  async sessionUnarchive_unstable(
    params: UnarchiveSessionRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/session/unarchive", params);
  }

  async sourcesCreate_unstable(
    params: CreateSourceRequest_unstable,
  ): Promise<CreateSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/create",
      params,
    );
    return zCreateSourceResponse_unstable.parse(
      raw,
    ) as CreateSourceResponse_unstable;
  }

  async sourcesList_unstable(
    params: ListSourcesRequest_unstable,
  ): Promise<ListSourcesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/list",
      params,
    );
    return zListSourcesResponse_unstable.parse(
      raw,
    ) as ListSourcesResponse_unstable;
  }

  async sourcesUpdate_unstable(
    params: UpdateSourceRequest_unstable,
  ): Promise<UpdateSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/update",
      params,
    );
    return zUpdateSourceResponse_unstable.parse(
      raw,
    ) as UpdateSourceResponse_unstable;
  }

  async sourcesDelete_unstable(
    params: DeleteSourceRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/sources/delete", params);
  }

  async sourcesExport_unstable(
    params: ExportSourceRequest_unstable,
  ): Promise<ExportSourceResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/export",
      params,
    );
    return zExportSourceResponse_unstable.parse(
      raw,
    ) as ExportSourceResponse_unstable;
  }

  async sourcesImport_unstable(
    params: ImportSourcesRequest_unstable,
  ): Promise<ImportSourcesResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/sources/import",
      params,
    );
    return zImportSourcesResponse_unstable.parse(
      raw,
    ) as ImportSourcesResponse_unstable;
  }

  async dictationTranscribe_unstable(
    params: DictationTranscribeRequest_unstable,
  ): Promise<DictationTranscribeResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/transcribe",
      params,
    );
    return zDictationTranscribeResponse_unstable.parse(
      raw,
    ) as DictationTranscribeResponse_unstable;
  }

  async dictationConfig_unstable(
    params: DictationConfigRequest_unstable,
  ): Promise<DictationConfigResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/config",
      params,
    );
    return zDictationConfigResponse_unstable.parse(
      raw,
    ) as DictationConfigResponse_unstable;
  }

  async dictationSecretSave_unstable(
    params: DictationSecretSaveRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod("_goose/unstable/dictation/secret/save", params);
  }

  async dictationSecretDelete_unstable(
    params: DictationSecretDeleteRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/secret/delete",
      params,
    );
  }

  async dictationModelsList_unstable(
    params: DictationModelsListRequest_unstable,
  ): Promise<DictationModelsListResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/models/list",
      params,
    );
    return zDictationModelsListResponse_unstable.parse(
      raw,
    ) as DictationModelsListResponse_unstable;
  }

  async dictationModelsDownload_unstable(
    params: DictationModelDownloadRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/download",
      params,
    );
  }

  async dictationModelsDownloadProgress_unstable(
    params: DictationModelDownloadProgressRequest_unstable,
  ): Promise<DictationModelDownloadProgressResponse_unstable> {
    const raw = await this.conn.extMethod(
      "_goose/unstable/dictation/models/download/progress",
      params,
    );
    return zDictationModelDownloadProgressResponse_unstable.parse(
      raw,
    ) as DictationModelDownloadProgressResponse_unstable;
  }

  async dictationModelsCancel_unstable(
    params: DictationModelCancelRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/cancel",
      params,
    );
  }

  async dictationModelsDelete_unstable(
    params: DictationModelDeleteRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/delete",
      params,
    );
  }

  async dictationModelsSelect_unstable(
    params: DictationModelSelectRequest_unstable,
  ): Promise<void> {
    await this.conn.extMethod(
      "_goose/unstable/dictation/models/select",
      params,
    );
  }
}
