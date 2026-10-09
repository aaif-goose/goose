use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    CreateElicitationRequest, CreateElicitationResponse, ElicitationAction as AcpElicitationAction,
    ElicitationFormMode, ElicitationSchema, ElicitationSessionScope, Meta, SessionId,
    CLIENT_METHOD_NAMES,
};
use agent_client_protocol::{
    Client, ConnectionTo, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, UntypedMessage,
};
use tracing::warn;

use crate::action_required_manager::ElicitationOutcome;
use crate::session::SessionManager;

impl super::AcpConnection {
    pub(super) async fn handle_form_elicitation(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: &SessionId,
        elicitation_id: &str,
        message: &str,
        requested_schema: &serde_json::Value,
        meta: Meta,
    ) -> Result<(), agent_client_protocol::Error> {
        if self.supports_acp_elicitation() {
            self.send_form_elicitation(
                cx,
                session_id,
                elicitation_id,
                message,
                requested_schema,
                meta,
            )
            .await?;
        } else {
            warn!(
                session_id = %session_id.0.as_ref(),
                elicitation_id = %elicitation_id,
                "ACP client does not support form elicitation"
            );
            self.cancel_form_elicitation(session_id.0.as_ref(), elicitation_id)
                .await;
        }

        Ok(())
    }

    async fn send_form_elicitation(
        &self,
        cx: &ConnectionTo<Client>,
        session_id: &SessionId,
        elicitation_id: &str,
        message: &str,
        requested_schema: &serde_json::Value,
        meta: Meta,
    ) -> Result<(), agent_client_protocol::Error> {
        let session_id = session_id.0.as_ref().to_string();
        let elicitation_id = elicitation_id.to_string();
        if requested_schema
            .get("url")
            .and_then(|url| url.as_str())
            .is_some()
        {
            warn!(
                session_id = %session_id,
                elicitation_id = %elicitation_id,
                "ACP URL elicitation is not supported"
            );
            record_acp_elicitation_response(
                &self.session_manager,
                &session_id,
                &elicitation_id,
                ElicitationOutcome::Cancel,
            )
            .await;
            return Ok(());
        }

        let requested_schema: ElicitationSchema =
            match serde_json::from_value(requested_schema.clone()) {
                Ok(schema) => schema,
                Err(error) => {
                    record_acp_elicitation_response(
                        &self.session_manager,
                        &session_id,
                        &elicitation_id,
                        ElicitationOutcome::Cancel,
                    )
                    .await;
                    return Err(agent_client_protocol::Error::internal_error()
                        .data(format!("Failed to parse ACP elicitation schema: {error}")));
                }
            };
        let request = CreateElicitationRequest::new(
            ElicitationFormMode::new(
                ElicitationSessionScope::new(session_id.clone()),
                requested_schema,
            ),
            message.to_string(),
        )
        .meta(meta);

        let callback_session_manager = Arc::clone(&self.session_manager);
        let callback_session_id = session_id.clone();
        let callback_elicitation_id = elicitation_id.clone();
        if let Err(error) = cx
            .send_request(CreateElicitationRequestMessage(request))
            .on_receiving_result(move |result| async move {
                let response = match result {
                    Ok(response) => elicitation_response_from_acp(response.0),
                    Err(error) => {
                        warn!(
                            error = %error,
                            session_id = %callback_session_id,
                            elicitation_id = %callback_elicitation_id,
                            "ACP elicitation request failed"
                        );
                        ElicitationOutcome::Cancel
                    }
                };

                record_acp_elicitation_response(
                    &callback_session_manager,
                    &callback_session_id,
                    &callback_elicitation_id,
                    response,
                )
                .await;

                Ok(())
            })
        {
            record_acp_elicitation_response(
                &self.session_manager,
                &session_id,
                &elicitation_id,
                ElicitationOutcome::Cancel,
            )
            .await;
            return Err(error);
        }

        Ok(())
    }

    async fn cancel_form_elicitation(&self, session_id: &str, elicitation_id: &str) {
        record_acp_elicitation_response(
            &self.session_manager,
            session_id,
            elicitation_id,
            ElicitationOutcome::Cancel,
        )
        .await;
    }
}

#[derive(Debug, Clone)]
struct CreateElicitationRequestMessage(CreateElicitationRequest);

impl JsonRpcMessage for CreateElicitationRequestMessage {
    fn matches_method(method: &str) -> bool {
        method == CLIENT_METHOD_NAMES.elicitation_create
    }

    fn method(&self) -> &str {
        CLIENT_METHOD_NAMES.elicitation_create
    }

    fn to_untyped_message(&self) -> Result<UntypedMessage, agent_client_protocol::Error> {
        UntypedMessage::new(CLIENT_METHOD_NAMES.elicitation_create, &self.0)
    }

    fn parse_message(
        method: &str,
        params: &impl serde::Serialize,
    ) -> Result<Self, agent_client_protocol::Error> {
        if !Self::matches_method(method) {
            return Err(agent_client_protocol::Error::method_not_found());
        }

        Ok(Self(agent_client_protocol::util::json_cast_params(params)?))
    }
}

impl JsonRpcRequest for CreateElicitationRequestMessage {
    type Response = CreateElicitationResponseMessage;
}

#[derive(Debug, Clone)]
struct CreateElicitationResponseMessage(CreateElicitationResponse);

impl JsonRpcResponse for CreateElicitationResponseMessage {
    fn into_json(self, _method: &str) -> Result<serde_json::Value, agent_client_protocol::Error> {
        serde_json::to_value(self.0).map_err(agent_client_protocol::Error::into_internal_error)
    }

    fn from_value(
        _method: &str,
        value: serde_json::Value,
    ) -> Result<Self, agent_client_protocol::Error> {
        Ok(Self(agent_client_protocol::util::json_cast(&value)?))
    }
}

pub(super) fn client_supports_form_elicitation(
    args: &agent_client_protocol::schema::v1::InitializeRequest,
) -> bool {
    args.client_capabilities
        .elicitation
        .as_ref()
        .and_then(|elicitation| elicitation.form.as_ref())
        .is_some()
}

fn elicitation_response_from_acp(response: CreateElicitationResponse) -> ElicitationOutcome {
    match response.action {
        AcpElicitationAction::Accept(action) => {
            let content = serde_json::to_value(action.content.unwrap_or_default())
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
            ElicitationOutcome::Accept(content)
        }
        AcpElicitationAction::Decline => ElicitationOutcome::Decline,
        AcpElicitationAction::Cancel => ElicitationOutcome::Cancel,
        action => {
            warn!(?action, "Unsupported ACP elicitation action");
            ElicitationOutcome::Cancel
        }
    }
}

async fn record_acp_elicitation_response(
    session_manager: &SessionManager,
    session_id: &str,
    elicitation_id: &str,
    response: ElicitationOutcome,
) {
    if let Err(error) = crate::elicitation::complete_elicitation_with_generated_message(
        session_manager,
        session_id,
        elicitation_id,
        response,
    )
    .await
    {
        warn!(
            error = %error,
            session_id = %session_id,
            elicitation_id = %elicitation_id,
            "Failed to record ACP elicitation response"
        );
    }
}
