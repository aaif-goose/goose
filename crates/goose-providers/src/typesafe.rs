use crate::api_client::ApiClient;
use crate::decision::{DecisionProvider, DecisionRequest, DecisionResponse};
use crate::errors::ProviderError;
use crate::http_status::read_json_response;
use crate::openai_compatible::handle_status;
use async_trait::async_trait;

pub const TYPESAFE_DEFAULT_HOST: &str = "https://api.typesafe.ai";
pub const TYPESAFE_DEFAULT_MODEL: &str = "jev-latest";

pub struct TypeSafeProvider {
    api_client: ApiClient,
}

impl TypeSafeProvider {
    pub fn new(api_client: ApiClient) -> Self {
        Self { api_client }
    }
}

#[async_trait]
impl DecisionProvider for TypeSafeProvider {
    async fn create_decision(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        let payload = serde_json::to_value(request).map_err(|error| {
            ProviderError::RequestFailed(format!("Failed to serialize decision request: {error}"))
        })?;
        let response = self
            .api_client
            .request("v1/systemone")
            .response_post(&payload)
            .await?;
        let response = handle_status(response).await?;
        read_json_response(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::AuthMethod;
    use crate::decision::{DecisionAnswer, DecisionQuestion};
    use serde_json::json;
    use std::collections::HashMap;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn creates_system_one_decision() {
        let server = MockServer::start().await;
        let request = DecisionRequest {
            model: TYPESAFE_DEFAULT_MODEL.to_string(),
            state: json!("Please help ASAP"),
            questions: HashMap::from([(
                "urgent".to_string(),
                DecisionQuestion::Noul {
                    instructions: "Does this express urgency?".to_string(),
                    criteria: None,
                },
            )]),
        };

        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_json(&request))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "jev-latest",
                "answers": {"urgent": {"type": "noul", "noul": 0.99}},
                "usage": {"input_tokens": 12, "output_tokens": 3}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = TypeSafeProvider::new(
            ApiClient::new_with_tls(server.uri(), AuthMethod::BearerToken("key".into()), None)
                .unwrap(),
        );
        let response = provider.create_decision(&request).await.unwrap();

        assert_eq!(
            response.answers["urgent"],
            DecisionAnswer::Noul { noul: 0.99 }
        );
    }
}
