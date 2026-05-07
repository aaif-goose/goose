use super::*;
use goose_acp_macros::custom_methods;

#[custom_methods]
impl GooseAcpAgent {
    pub async fn dispatch_custom_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, agent_client_protocol::Error> {
        self.handle_custom_request(method, params).await
    }

    #[custom_method(AddExtensionRequest)]
    async fn dispatch_add_extension(
        &self,
        req: AddExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_add_extension(req).await
    }

    #[custom_method(RemoveExtensionRequest)]
    async fn dispatch_remove_extension(
        &self,
        req: RemoveExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_remove_extension(req).await
    }

    #[custom_method(GetToolsRequest)]
    async fn dispatch_get_tools(
        &self,
        req: GetToolsRequest,
    ) -> Result<GetToolsResponse, agent_client_protocol::Error> {
        self.on_get_tools(req).await
    }

    #[custom_method(GooseToolCallRequest)]
    async fn dispatch_call_tool(
        &self,
        req: GooseToolCallRequest,
    ) -> Result<GooseToolCallResponse, agent_client_protocol::Error> {
        self.on_call_tool(req).await
    }

    #[custom_method(ReadResourceRequest)]
    async fn dispatch_read_resource(
        &self,
        req: ReadResourceRequest,
    ) -> Result<ReadResourceResponse, agent_client_protocol::Error> {
        self.on_read_resource(req).await
    }

    #[custom_method(UpdateWorkingDirRequest)]
    async fn dispatch_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_update_working_dir(req).await
    }

    #[custom_method(DeleteSessionRequest)]
    async fn dispatch_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_delete_session(req).await
    }

    #[custom_method(GetExtensionsRequest)]
    async fn dispatch_get_extensions(
        &self,
    ) -> Result<GetExtensionsResponse, agent_client_protocol::Error> {
        self.on_get_extensions().await
    }

    #[custom_method(AddConfigExtensionRequest)]
    async fn dispatch_add_config_extension(
        &self,
        req: AddConfigExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_add_config_extension(req).await
    }

    #[custom_method(RemoveConfigExtensionRequest)]
    async fn dispatch_remove_config_extension(
        &self,
        req: RemoveConfigExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_remove_config_extension(req).await
    }

    #[custom_method(ToggleConfigExtensionRequest)]
    async fn dispatch_toggle_config_extension(
        &self,
        req: ToggleConfigExtensionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_toggle_config_extension(req).await
    }

    #[custom_method(GetSessionExtensionsRequest)]
    async fn dispatch_get_session_extensions(
        &self,
        req: GetSessionExtensionsRequest,
    ) -> Result<GetSessionExtensionsResponse, agent_client_protocol::Error> {
        self.on_get_session_extensions(req).await
    }

    #[custom_method(ListProvidersRequest)]
    async fn dispatch_list_providers(
        &self,
        req: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, agent_client_protocol::Error> {
        self.on_list_providers(req).await
    }

    #[custom_method(ProviderCatalogListRequest)]
    async fn dispatch_list_provider_catalog(
        &self,
        req: ProviderCatalogListRequest,
    ) -> Result<ProviderCatalogListResponse, agent_client_protocol::Error> {
        self.on_list_provider_catalog(req).await
    }

    #[custom_method(ProviderSetupCatalogListRequest)]
    async fn dispatch_list_provider_setup_catalog(
        &self,
        req: ProviderSetupCatalogListRequest,
    ) -> Result<ProviderSetupCatalogListResponse, agent_client_protocol::Error> {
        self.on_list_provider_setup_catalog(req).await
    }

    #[custom_method(ProviderCatalogTemplateRequest)]
    async fn dispatch_get_provider_catalog_template(
        &self,
        req: ProviderCatalogTemplateRequest,
    ) -> Result<ProviderCatalogTemplateResponse, agent_client_protocol::Error> {
        self.on_get_provider_catalog_template(req).await
    }

    #[custom_method(CustomProviderCreateRequest)]
    async fn dispatch_create_custom_provider(
        &self,
        req: CustomProviderCreateRequest,
    ) -> Result<CustomProviderCreateResponse, agent_client_protocol::Error> {
        self.on_create_custom_provider(req).await
    }

    #[custom_method(CustomProviderReadRequest)]
    async fn dispatch_read_custom_provider(
        &self,
        req: CustomProviderReadRequest,
    ) -> Result<CustomProviderReadResponse, agent_client_protocol::Error> {
        self.on_read_custom_provider(req).await
    }

    #[custom_method(CustomProviderUpdateRequest)]
    async fn dispatch_update_custom_provider(
        &self,
        req: CustomProviderUpdateRequest,
    ) -> Result<CustomProviderUpdateResponse, agent_client_protocol::Error> {
        self.on_update_custom_provider(req).await
    }

    #[custom_method(CustomProviderDeleteRequest)]
    async fn dispatch_delete_custom_provider(
        &self,
        req: CustomProviderDeleteRequest,
    ) -> Result<CustomProviderDeleteResponse, agent_client_protocol::Error> {
        self.on_delete_custom_provider(req).await
    }

    #[custom_method(RefreshProviderInventoryRequest)]
    async fn dispatch_refresh_provider_inventory(
        &self,
        req: RefreshProviderInventoryRequest,
    ) -> Result<RefreshProviderInventoryResponse, agent_client_protocol::Error> {
        self.on_refresh_provider_inventory(req).await
    }

    #[custom_method(ProviderConfigReadRequest)]
    async fn dispatch_read_provider_config(
        &self,
        req: ProviderConfigReadRequest,
    ) -> Result<ProviderConfigReadResponse, agent_client_protocol::Error> {
        self.on_read_provider_config(req).await
    }

    #[custom_method(ProviderConfigStatusRequest)]
    async fn dispatch_provider_config_status(
        &self,
        req: ProviderConfigStatusRequest,
    ) -> Result<ProviderConfigStatusResponse, agent_client_protocol::Error> {
        self.on_provider_config_status(req).await
    }

    #[custom_method(ProviderConfigSaveRequest)]
    async fn dispatch_save_provider_config(
        &self,
        req: ProviderConfigSaveRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_save_provider_config(req).await
    }

    #[custom_method(ProviderConfigDeleteRequest)]
    async fn dispatch_delete_provider_config(
        &self,
        req: ProviderConfigDeleteRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_delete_provider_config(req).await
    }

    #[custom_method(ProviderConfigAuthenticateRequest)]
    async fn dispatch_authenticate_provider_config(
        &self,
        req: ProviderConfigAuthenticateRequest,
    ) -> Result<ProviderConfigChangeResponse, agent_client_protocol::Error> {
        self.on_authenticate_provider_config(req).await
    }

    #[custom_method(PreferencesReadRequest)]
    async fn dispatch_preferences_read(
        &self,
        req: PreferencesReadRequest,
    ) -> Result<PreferencesReadResponse, agent_client_protocol::Error> {
        self.on_preferences_read(req).await
    }

    #[custom_method(PreferencesSaveRequest)]
    async fn dispatch_preferences_save(
        &self,
        req: PreferencesSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_preferences_save(req).await
    }

    #[custom_method(PreferencesRemoveRequest)]
    async fn dispatch_preferences_remove(
        &self,
        req: PreferencesRemoveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_preferences_remove(req).await
    }

    #[custom_method(DefaultsReadRequest)]
    async fn dispatch_defaults_read(
        &self,
        req: DefaultsReadRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        self.on_defaults_read(req).await
    }

    #[custom_method(DefaultsSaveRequest)]
    async fn dispatch_defaults_save(
        &self,
        req: DefaultsSaveRequest,
    ) -> Result<DefaultsReadResponse, agent_client_protocol::Error> {
        self.on_defaults_save(req).await
    }

    #[custom_method(OnboardingImportScanRequest)]
    async fn dispatch_onboarding_import_scan(
        &self,
        req: OnboardingImportScanRequest,
    ) -> Result<OnboardingImportScanResponse, agent_client_protocol::Error> {
        self.on_onboarding_import_scan(req).await
    }

    #[custom_method(OnboardingImportApplyRequest)]
    async fn dispatch_onboarding_import_apply(
        &self,
        req: OnboardingImportApplyRequest,
    ) -> Result<OnboardingImportApplyResponse, agent_client_protocol::Error> {
        self.on_onboarding_import_apply(req).await
    }

    #[custom_method(ExportSessionRequest)]
    async fn dispatch_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        self.on_export_session(req).await
    }

    #[custom_method(ImportSessionRequest)]
    async fn dispatch_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        self.on_import_session(req).await
    }

    #[custom_method(UpdateSessionProjectRequest)]
    async fn dispatch_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_update_session_project(req).await
    }

    #[custom_method(RenameSessionRequest)]
    async fn dispatch_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_rename_session(req).await
    }

    #[custom_method(ArchiveSessionRequest)]
    async fn dispatch_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_archive_session(req).await
    }

    #[custom_method(UnarchiveSessionRequest)]
    async fn dispatch_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_unarchive_session(req).await
    }

    #[custom_method(CreateSourceRequest)]
    async fn dispatch_create_source(
        &self,
        req: CreateSourceRequest,
    ) -> Result<CreateSourceResponse, agent_client_protocol::Error> {
        self.on_create_source(req).await
    }

    #[custom_method(ListSourcesRequest)]
    async fn dispatch_list_sources(
        &self,
        req: ListSourcesRequest,
    ) -> Result<ListSourcesResponse, agent_client_protocol::Error> {
        self.on_list_sources(req).await
    }

    #[custom_method(UpdateSourceRequest)]
    async fn dispatch_update_source(
        &self,
        req: UpdateSourceRequest,
    ) -> Result<UpdateSourceResponse, agent_client_protocol::Error> {
        self.on_update_source(req).await
    }

    #[custom_method(DeleteSourceRequest)]
    async fn dispatch_delete_source(
        &self,
        req: DeleteSourceRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_delete_source(req).await
    }

    #[custom_method(ExportSourceRequest)]
    async fn dispatch_export_source(
        &self,
        req: ExportSourceRequest,
    ) -> Result<ExportSourceResponse, agent_client_protocol::Error> {
        self.on_export_source(req).await
    }

    #[custom_method(ImportSourcesRequest)]
    async fn dispatch_import_sources(
        &self,
        req: ImportSourcesRequest,
    ) -> Result<ImportSourcesResponse, agent_client_protocol::Error> {
        self.on_import_sources(req).await
    }

    #[custom_method(DictationTranscribeRequest)]
    async fn dispatch_dictation_transcribe(
        &self,
        req: DictationTranscribeRequest,
    ) -> Result<DictationTranscribeResponse, agent_client_protocol::Error> {
        self.on_dictation_transcribe(req).await
    }

    #[custom_method(DictationConfigRequest)]
    async fn dispatch_dictation_config(
        &self,
        _req: DictationConfigRequest,
    ) -> Result<DictationConfigResponse, agent_client_protocol::Error> {
        self.on_dictation_config(_req).await
    }

    #[custom_method(DictationSecretSaveRequest)]
    async fn dispatch_dictation_secret_save(
        &self,
        req: DictationSecretSaveRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_secret_save(req).await
    }

    #[custom_method(DictationSecretDeleteRequest)]
    async fn dispatch_dictation_secret_delete(
        &self,
        req: DictationSecretDeleteRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_secret_delete(req).await
    }

    #[custom_method(DictationModelsListRequest)]
    async fn dispatch_dictation_models_list(
        &self,
        _req: DictationModelsListRequest,
    ) -> Result<DictationModelsListResponse, agent_client_protocol::Error> {
        self.on_dictation_models_list(_req).await
    }

    #[custom_method(DictationModelDownloadRequest)]
    async fn dispatch_dictation_model_download(
        &self,
        _req: DictationModelDownloadRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_model_download(_req).await
    }

    #[custom_method(DictationModelDownloadProgressRequest)]
    async fn dispatch_dictation_model_download_progress(
        &self,
        _req: DictationModelDownloadProgressRequest,
    ) -> Result<DictationModelDownloadProgressResponse, agent_client_protocol::Error> {
        self.on_dictation_model_download_progress(_req).await
    }

    #[custom_method(DictationModelCancelRequest)]
    async fn dispatch_dictation_model_cancel(
        &self,
        _req: DictationModelCancelRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_model_cancel(_req).await
    }

    #[custom_method(DictationModelDeleteRequest)]
    async fn dispatch_dictation_model_delete(
        &self,
        _req: DictationModelDeleteRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_model_delete(_req).await
    }

    #[custom_method(DictationModelSelectRequest)]
    async fn dispatch_dictation_model_select(
        &self,
        req: DictationModelSelectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.on_dictation_model_select(req).await
    }
}
