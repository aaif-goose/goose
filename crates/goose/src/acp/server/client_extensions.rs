use super::*;
use crate::client_extensions::{
    client_extensions_dir, disable_client_extension, enable_client_extension,
    install_client_extension, list_client_extensions, net, read_client_extension_main, storage,
    uninstall_client_extension, ClientExtensionSource, ClientExtensionSummary,
};
use std::path::Path;

fn to_acp_error(error: anyhow::Error) -> agent_client_protocol::Error {
    agent_client_protocol::Error::invalid_params().data(format!("{error:#}"))
}

fn to_info(summary: ClientExtensionSummary) -> ClientExtensionInfo {
    ClientExtensionInfo {
        id: summary.id,
        version: summary.version,
        directory: summary.directory.display().to_string(),
        source: match summary.source {
            ClientExtensionSource::Installed => ClientExtensionSourceKind::Installed,
            ClientExtensionSource::Dev => ClientExtensionSourceKind::Dev,
        },
        enabled: summary.enabled,
        manifest: summary.manifest,
    }
}

fn list_response() -> Result<ClientExtensionsListResponse, agent_client_protocol::Error> {
    let extensions = list_client_extensions()
        .map_err(to_acp_error)?
        .into_iter()
        .map(to_info)
        .collect();
    Ok(ClientExtensionsListResponse {
        install_dir: client_extensions_dir().display().to_string(),
        extensions,
    })
}

impl GooseAcpAgent {
    pub(super) async fn on_client_extensions_list(
        &self,
        _req: ClientExtensionsListRequest,
    ) -> Result<ClientExtensionsListResponse, agent_client_protocol::Error> {
        list_response()
    }

    pub(super) async fn on_client_extensions_install(
        &self,
        req: ClientExtensionsInstallRequest,
    ) -> Result<ClientExtensionsInstallResponse, agent_client_protocol::Error> {
        let install =
            install_client_extension(Path::new(&req.source_path)).map_err(to_acp_error)?;
        let listing = list_response()?;
        Ok(ClientExtensionsInstallResponse {
            installed_id: install.id,
            install_dir: listing.install_dir,
            extensions: listing.extensions,
        })
    }

    pub(super) async fn on_client_extensions_set_enabled(
        &self,
        req: ClientExtensionsSetEnabledRequest,
    ) -> Result<ClientExtensionsListResponse, agent_client_protocol::Error> {
        let result = if req.enabled {
            enable_client_extension(&req.id)
        } else {
            disable_client_extension(&req.id)
        };
        result.map_err(to_acp_error)?;
        list_response()
    }

    pub(super) async fn on_client_extensions_uninstall(
        &self,
        req: ClientExtensionsUninstallRequest,
    ) -> Result<ClientExtensionsListResponse, agent_client_protocol::Error> {
        uninstall_client_extension(&req.id).map_err(to_acp_error)?;
        list_response()
    }

    pub(super) async fn on_client_extensions_read_main(
        &self,
        req: ClientExtensionsReadMainRequest,
    ) -> Result<ClientExtensionsReadMainResponse, agent_client_protocol::Error> {
        let html = read_client_extension_main(&req.id).map_err(to_acp_error)?;
        Ok(ClientExtensionsReadMainResponse { html })
    }

    pub(super) async fn on_client_extension_storage_get(
        &self,
        req: ClientExtensionStorageGetRequest,
    ) -> Result<ClientExtensionStorageGetResponse, agent_client_protocol::Error> {
        let value = storage::get(&req.extension_id, &req.key).map_err(to_acp_error)?;
        Ok(ClientExtensionStorageGetResponse { value })
    }

    pub(super) async fn on_client_extension_storage_set(
        &self,
        req: ClientExtensionStorageSetRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        storage::set(&req.extension_id, &req.key, req.value).map_err(to_acp_error)?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_client_extension_storage_delete(
        &self,
        req: ClientExtensionStorageDeleteRequest,
    ) -> Result<ClientExtensionStorageDeleteResponse, agent_client_protocol::Error> {
        let existed = storage::delete(&req.extension_id, &req.key).map_err(to_acp_error)?;
        Ok(ClientExtensionStorageDeleteResponse { existed })
    }

    pub(super) async fn on_client_extension_storage_keys(
        &self,
        req: ClientExtensionStorageKeysRequest,
    ) -> Result<ClientExtensionStorageKeysResponse, agent_client_protocol::Error> {
        let keys = storage::keys(&req.extension_id).map_err(to_acp_error)?;
        Ok(ClientExtensionStorageKeysResponse { keys })
    }

    pub(super) async fn on_client_extension_net_fetch(
        &self,
        req: ClientExtensionNetFetchRequest,
    ) -> Result<ClientExtensionNetFetchResponse, agent_client_protocol::Error> {
        let result = net::fetch(
            &req.extension_id,
            &req.url,
            req.method.as_deref(),
            req.headers,
            req.body,
        )
        .await
        .map_err(to_acp_error)?;
        Ok(ClientExtensionNetFetchResponse {
            ok: result.ok,
            status: result.status,
            headers: result.headers,
            text: result.text,
        })
    }
}
