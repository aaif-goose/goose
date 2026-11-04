//! Shared paginated discovery and tool exposure for standalone and hosted clients.
use super::Client;
use goose_provider_types::maybe_send::MaybeSync;
use rmcp::{
    model::{Prompt, Resource, Tool},
    ServiceError,
};
use std::{collections::HashSet, sync::Arc};
use tokio_util::sync::CancellationToken;

macro_rules! discovery {
    ($function:ident, $method:ident, $field:ident, $item:ty) => {
        pub async fn $function<C: MaybeSync>(
            client: &dyn Client<C>,
            scope: &str,
            cancel: CancellationToken,
        ) -> Result<Vec<$item>, ServiceError> {
            let mut items = Vec::new();
            let mut cursor = None;
            let mut seen = HashSet::new();
            loop {
                let page = client.$method(scope, cursor, cancel.clone()).await?;
                items.extend(page.$field);
                cursor = page.next_cursor;
                match &cursor {
                    None => return Ok(items),
                    Some(value) if !seen.insert(value.clone()) => {
                        return Err(ServiceError::UnexpectedResponse)
                    }
                    _ => {}
                }
            }
        }
    };
}
discovery!(tools, list_tools, tools, Tool);
discovery!(resources, list_resources, resources, Resource);
discovery!(prompts, list_prompts, prompts, Prompt);

pub fn expose_tool(mut tool: Tool, owner: &str, unprefixed: bool) -> Tool {
    if !unprefixed {
        tool.name = format!("{owner}__{}", tool.name).into();
    }
    let mut schema = (*tool.input_schema).clone();
    if super::schema::normalize_input_schema(&mut schema) {
        tool.input_schema = Arc::new(schema);
    }
    tool
}
