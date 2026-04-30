use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_create_source(
        &self,
        req: CreateSourceRequest,
    ) -> Result<CreateSourceResponse, sacp::Error> {
        let source = crate::sources::create_source(
            req.source_type,
            &req.name,
            &req.description,
            &req.content,
            req.global,
            req.project_dir.as_deref(),
        )?;
        Ok(CreateSourceResponse { source })
    }

    pub(super) async fn on_list_sources(
        &self,
        req: ListSourcesRequest,
    ) -> Result<ListSourcesResponse, sacp::Error> {
        let sources = crate::sources::list_sources(req.source_type, req.project_dir.as_deref())?;
        Ok(ListSourcesResponse { sources })
    }

    pub(super) async fn on_update_source(
        &self,
        req: UpdateSourceRequest,
    ) -> Result<UpdateSourceResponse, sacp::Error> {
        let source = crate::sources::update_source(
            req.source_type,
            &req.path,
            &req.name,
            &req.description,
            &req.content,
        )?;
        Ok(UpdateSourceResponse { source })
    }

    pub(super) async fn on_delete_source(
        &self,
        req: DeleteSourceRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        crate::sources::delete_source(req.source_type, &req.path)?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_source(
        &self,
        req: ExportSourceRequest,
    ) -> Result<ExportSourceResponse, sacp::Error> {
        let (json, filename) = crate::sources::export_source(req.source_type, &req.path)?;
        Ok(ExportSourceResponse { json, filename })
    }

    pub(super) async fn on_import_sources(
        &self,
        req: ImportSourcesRequest,
    ) -> Result<ImportSourcesResponse, sacp::Error> {
        let sources =
            crate::sources::import_sources(&req.data, req.global, req.project_dir.as_deref())?;
        Ok(ImportSourcesResponse { sources })
    }
}
