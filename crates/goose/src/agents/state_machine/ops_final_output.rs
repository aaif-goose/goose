use oi_kit_agent::final_output::FinalOutputSchemaSource;
use serde_json::Value;

use crate::session::Session;

pub(crate) fn recipe_unsupported_message(provider_name: &str) -> String {
    format!(
        "This recipe declares a structured `response`, but provider `{provider_name}` can't \
         support it because it never receives goose's built-in `final_output` tool, so the \
         model can never satisfy this recipe. Remove the entire `response` block from the recipe \
         or run it with a different provider."
    )
}

pub(crate) struct GooseFinalOutputSchemaSource;

impl FinalOutputSchemaSource<Session> for GooseFinalOutputSchemaSource {
    fn json_schema(&self, session: &Session) -> Option<Value> {
        session
            .recipe
            .as_ref()?
            .response
            .as_ref()
            .map(|response| response.json_schema.clone())
    }
}
