use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_create_source(
        &self,
        req: CreateSourceRequest,
    ) -> Result<CreateSourceResponse, agent_client_protocol::Error> {
        let project_dir = match (&req.project_id, &req.project_dir) {
            (Some(pid), _) if !req.global => {
                let dirs = crate::sources::project_working_dirs(pid);
                Some(dirs.into_iter().next().ok_or_else(|| {
                    agent_client_protocol::Error::invalid_params().data(format!(
                        "Project \"{pid}\" has no working directories configured"
                    ))
                })?)
            }
            (_, Some(pd)) => Some(pd.clone()),
            _ => None,
        };
        let source = crate::sources::create_source(
            req.source_type,
            &req.name,
            &req.description,
            &req.content,
            req.global,
            project_dir.as_deref(),
            req.properties,
        )?;
        Ok(CreateSourceResponse { source })
    }

    pub(super) async fn on_list_sources(
        &self,
        req: ListSourcesRequest,
    ) -> Result<ListSourcesResponse, agent_client_protocol::Error> {
        let sources = crate::sources::list_sources(
            req.source_type,
            req.project_dir.as_deref(),
            req.include_project_sources,
        )?;
        Ok(ListSourcesResponse { sources })
    }

    pub(super) async fn on_update_source(
        &self,
        req: UpdateSourceRequest,
    ) -> Result<UpdateSourceResponse, agent_client_protocol::Error> {
        let source = crate::sources::update_source(
            req.source_type,
            &req.path,
            &req.name,
            &req.description,
            &req.content,
            req.properties,
        )?;
        Ok(UpdateSourceResponse { source })
    }

    pub(super) async fn on_delete_source(
        &self,
        req: DeleteSourceRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        crate::sources::delete_source(req.source_type, &req.path)?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_source(
        &self,
        req: ExportSourceRequest,
    ) -> Result<ExportSourceResponse, agent_client_protocol::Error> {
        let (json, filename) = crate::sources::export_source(req.source_type, &req.path)?;
        Ok(ExportSourceResponse { json, filename })
    }

    pub(super) async fn on_import_sources(
        &self,
        req: ImportSourcesRequest,
    ) -> Result<ImportSourcesResponse, agent_client_protocol::Error> {
        let sources =
            crate::sources::import_sources(&req.data, req.global, req.project_dir.as_deref())?;
        Ok(ImportSourcesResponse { sources })
    }
}
