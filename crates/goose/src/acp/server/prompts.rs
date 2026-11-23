use super::*;
use crate::prompt_template::{get_template, list_templates, reset_template, save_template};

impl AcpConnection {
    pub(super) async fn on_list_prompts(
        &self,
        _req: ListPromptsRequest,
    ) -> Result<ListPromptsResponse, agent_client_protocol::Error> {
        let prompts = list_templates()
            .into_iter()
            .map(prompt_template_to_entry)
            .collect();

        Ok(ListPromptsResponse { prompts })
    }

    pub(super) async fn on_get_prompt(
        &self,
        req: GetPromptRequest,
    ) -> Result<GetPromptResponse, agent_client_protocol::Error> {
        let template = get_template(&req.name).ok_or_else(|| prompt_not_found(&req.name))?;
        let content = template
            .user_content
            .as_ref()
            .unwrap_or(&template.default_content)
            .clone();

        Ok(GetPromptResponse {
            name: template.name,
            content,
            default_content: template.default_content,
            is_customized: template.is_customized,
        })
    }

    pub(super) async fn on_save_prompt(
        &self,
        req: SavePromptRequest,
    ) -> Result<PromptOperationResponse, agent_client_protocol::Error> {
        save_template(&req.name, &req.content).map_err(|err| prompt_io_error(&req.name, err))?;

        Ok(PromptOperationResponse {
            message: format!("Saved prompt: {}", req.name),
        })
    }

    pub(super) async fn on_reset_prompt(
        &self,
        req: ResetPromptRequest,
    ) -> Result<PromptOperationResponse, agent_client_protocol::Error> {
        reset_template(&req.name).map_err(|err| prompt_io_error(&req.name, err))?;

        Ok(PromptOperationResponse {
            message: format!("Reset prompt to default: {}", req.name),
        })
    }
}

fn prompt_template_to_entry(template: crate::prompt_template::Template) -> PromptTemplateEntry {
    PromptTemplateEntry {
        name: template.name,
        description: template.description,
        default_content: template.default_content,
        user_content: template.user_content,
        is_customized: template.is_customized,
    }
}

fn prompt_not_found(name: &str) -> agent_client_protocol::Error {
    agent_client_protocol::Error::invalid_params()
        .data(format!("Prompt template '{name}' not found"))
}

fn prompt_io_error(name: &str, err: std::io::Error) -> agent_client_protocol::Error {
    if err.kind() == std::io::ErrorKind::NotFound {
        prompt_not_found(name)
    } else {
        agent_client_protocol::Error::internal_error()
            .data(format!("Failed to update prompt '{name}': {err}"))
    }
}
