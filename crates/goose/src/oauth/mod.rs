mod persist;

pub use persist::GooseCredentialStore;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use minijinja::{context, Environment};
use oauth2::{Scope, TokenResponse};
use rmcp::transport::auth::{
    AuthError, AuthorizationRequest, CredentialStore, OAuthClientConfig, OAuthState,
    OAuthTokenResponse, StoredCredentials, WWWAuthenticateParams,
};
use rmcp::transport::AuthorizationManager;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, Mutex};
use tracing::warn;

const CALLBACK_TEMPLATE: &str = include_str!("oauth_callback.html");
const CLIENT_METADATA_URL: &str = "https://goose-docs.ai/oauth/client-metadata.json";
const DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS: u64 = 300;
const OAUTH_CALLBACK_TIMEOUT_ENV: &str = "GOOSE_OAUTH_CALLBACK_TIMEOUT_SECONDS";
/// Room for the discovery and token requests around the browser callback.
const AUTHORIZATION_LOCK_MARGIN: Duration = Duration::from_secs(60);

/// Pre-registered OAuth client supplied by a probe script, for servers whose
/// authorization server supports neither Dynamic Client Registration nor
/// Client ID Metadata Documents.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFlowConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_metadata_url: Option<String>,
}

#[derive(Clone)]
struct AppState {
    callback_receiver: Arc<Mutex<Option<oneshot::Sender<String>>>>,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
    iss: Option<String>,
}

fn resolve_oauth_callback_timeout(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS))
}

fn oauth_callback_timeout() -> Duration {
    let timeout = std::env::var(OAUTH_CALLBACK_TIMEOUT_ENV).ok();
    resolve_oauth_callback_timeout(timeout.as_deref())
}

fn render_oauth_callback(name: &str) -> String {
    Environment::new()
        .render_named_str(
            "oauth_callback.html",
            CALLBACK_TEMPLATE,
            context! { name => name },
        )
        .expect("failed to render OAuth callback")
}

fn announce_authorization_url(name: &str, authorization_url: &str) {
    warn!(
        "[OAuth:{}] If the browser did not open, authorize manually at: {}",
        name, authorization_url
    );
    eprintln!(
        "If the browser did not open, authorize {} at:\n  {}",
        name, authorization_url
    );
}

async fn complete_automatic_authorization(
    authorization_url: &str,
    redirect_uri: &str,
) -> Result<Option<String>, anyhow::Error> {
    if std::env::var_os("GOOSE_OAUTH_AUTOMATIC_CALLBACK").is_none() {
        return Ok(None);
    }

    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get(authorization_url)
        .send()
        .await?;
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .ok_or_else(|| anyhow::anyhow!("authorization response did not include Location"))?
        .to_str()?;
    let callback_url = url::Url::parse(location)?;
    let expected_redirect = url::Url::parse(redirect_uri)?;
    if callback_url.scheme() != expected_redirect.scheme()
        || callback_url.host_str() != expected_redirect.host_str()
        || callback_url.port_or_known_default() != expected_redirect.port_or_known_default()
        || callback_url.path() != expected_redirect.path()
    {
        anyhow::bail!("authorization response redirected to an unexpected callback URI");
    }
    Ok(Some(callback_url.to_string()))
}

async fn wait_for_callback(
    callback_receiver: oneshot::Receiver<String>,
    timeout_duration: Duration,
    name: &str,
    authorization_url: &str,
) -> Result<String, anyhow::Error> {
    match tokio::time::timeout(timeout_duration, callback_receiver).await {
        Ok(Ok(callback_url)) => Ok(callback_url),
        Ok(Err(e)) => Err(anyhow::anyhow!(
            "OAuth authorization for {} ended before the callback was received: {}",
            name,
            e
        )),
        Err(_) => {
            let message = format!(
                "OAuth authorization for {} timed out waiting for the local callback. \
                 Start the OAuth flow again and open this URL manually if the browser does not open: {}",
                name, authorization_url
            );
            warn!("[OAuth:{}] {}", name, message);
            Err(anyhow::anyhow!(message))
        }
    }
}

pub use crate::agents::extension_runtime::{
    RejectedRequest, StaticOAuthClientConfig, StoredCredentialsError,
};

/// Pre-registered client supplied through the environment, used by tools that
/// drive the flow without an extension config (`goose mcp-probe`, conformance
/// driver).
fn env_static_oauth_client() -> Option<StaticOAuthClientConfig> {
    Some(StaticOAuthClientConfig {
        client_id: std::env::var("GOOSE_MCP_OAUTH_CLIENT_ID").ok()?,
        client_secret: std::env::var("GOOSE_MCP_OAUTH_CLIENT_SECRET").ok(),
        scopes: Vec::new(),
    })
}

fn client_metadata_url() -> String {
    std::env::var("GOOSE_MCP_OAUTH_CLIENT_METADATA_URL")
        .unwrap_or_else(|_| CLIENT_METADATA_URL.to_string())
}

fn scope_set(scopes: &[String]) -> BTreeSet<&str> {
    scopes.iter().map(String::as_str).collect()
}

fn configured_scopes_changed(
    static_client: Option<&StaticOAuthClientConfig>,
    previous_requested_scopes: Option<&[String]>,
    granted_scopes: &[String],
) -> bool {
    let Some(client) = static_client else {
        return previous_requested_scopes.is_some();
    };

    match previous_requested_scopes {
        Some(previous) => scope_set(previous) != scope_set(&client.scopes),
        None => !scope_set(&client.scopes).is_subset(&scope_set(granted_scopes)),
    }
}

fn configured_client_changed(
    static_client: Option<&StaticOAuthClientConfig>,
    stored_client_id: &str,
) -> bool {
    static_client.is_some_and(|client| client.client_id != stored_client_id)
}

const REFRESH_BUFFER_SECS: u64 = 30;

fn access_token_needs_refresh(stored_credentials: &StoredCredentials) -> bool {
    let Some(token_response) = stored_credentials.token_response.as_ref() else {
        return true;
    };
    let (Some(expires_in), Some(received_at)) = (
        token_response.expires_in(),
        stored_credentials.token_received_at,
    ) else {
        return false;
    };
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(received_at);
    expires_in.as_secs().saturating_sub(elapsed) < REFRESH_BUFFER_SECS
}

fn configure_static_client(
    auth_manager: &mut AuthorizationManager,
    static_client: Option<&StaticOAuthClientConfig>,
    redirect_uri: &str,
) -> Result<(), AuthError> {
    let Some(client) = static_client else {
        return Ok(());
    };

    let mut config = OAuthClientConfig::new(client.client_id.clone(), redirect_uri.to_string());
    if let Some(secret) = &client.client_secret {
        config = config.with_client_secret(secret.clone());
    }
    auth_manager.configure_client(config)
}

fn restore_omitted_scopes(
    token_response: &mut OAuthTokenResponse,
    granted_scopes: &[String],
) -> bool {
    if token_response.scopes().is_some() || granted_scopes.is_empty() {
        return false;
    }

    token_response.set_scopes(Some(
        granted_scopes.iter().cloned().map(Scope::new).collect(),
    ));
    true
}

fn build_authorization_request(
    redirect_uri: String,
    static_client: Option<&StaticOAuthClientConfig>,
    challenge: Option<String>,
    mcp_server_url: &str,
    previously_granted_scopes: &[String],
) -> AuthorizationRequest {
    let mut request = AuthorizationRequest::new(redirect_uri).with_client_name("goose");
    match static_client {
        Some(client) => {
            request = request.with_preregistered_client(client.client_id.clone());
            if let Some(secret) = &client.client_secret {
                request = request.with_client_secret(secret.clone());
            }
            if !client.scopes.is_empty() {
                request = request.with_scopes(client.scopes.clone());
            }
        }
        None => {
            request = request.with_client_metadata_url(client_metadata_url());
        }
    }

    if let Some(challenge) = challenge {
        // SEP-2350: a re-authorization triggered by a scope challenge requests
        // the union of previously-granted scopes and the newly challenged
        // scopes. The fresh AuthorizationManager has no scope memory, so seed
        // the union from the stored grant.
        let mut scopes = previously_granted_scopes.to_vec();
        scopes.extend(
            static_client
                .into_iter()
                .flat_map(|client| client.scopes.iter().cloned()),
        );
        if let Ok(base_url) = url::Url::parse(mcp_server_url) {
            if let Some(challenged) = WWWAuthenticateParams::parse(&challenge, &base_url).scope {
                scopes.extend(challenged.split_whitespace().map(str::to_string));
            }
        }
        let mut seen = BTreeSet::new();
        scopes.retain(|scope| seen.insert(scope.clone()));
        if !scopes.is_empty() {
            request = request.with_scopes(scopes);
        }
        request = request.with_challenge(challenge);
    }

    request
}

#[derive(Debug, PartialEq)]
enum StoredGrantUse {
    AsIs,
    Refresh,
    Reauthorize,
}

fn stored_access_token(stored: &StoredCredentials) -> Option<&str> {
    stored
        .token_response
        .as_ref()
        .map(|token| token.access_token().secret().as_str())
}

/// The scopes a challenge demands when it reports `insufficient_scope`.
fn insufficient_scope_requirement(challenge: &str, mcp_server_url: &str) -> Option<Vec<String>> {
    let base_url = url::Url::parse(mcp_server_url).ok()?;
    let params = WWWAuthenticateParams::parse(challenge, &base_url);
    params.is_insufficient_scope().then(|| {
        params
            .scope
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    })
}

/// Decides, while holding the authorization lock, whether the grant in the
/// store can serve this request: it may be one another session saved while
/// this one waited.
fn stored_grant_use(
    stored: &StoredCredentials,
    static_client: Option<&StaticOAuthClientConfig>,
    previous_requested_scopes: Option<&[String]>,
    rejected: Option<&RejectedRequest>,
    mcp_server_url: &str,
) -> StoredGrantUse {
    if configured_scopes_changed(
        static_client,
        previous_requested_scopes,
        &stored.granted_scopes,
    ) || configured_client_changed(static_client, &stored.client_id)
    {
        return StoredGrantUse::Reauthorize;
    }

    if let Some(rejected) = rejected {
        // The transport already refreshed once before passing the refusal on,
        // so the stored token cannot be the answer if the server refused it.
        if rejected
            .refused_tokens
            .iter()
            .any(|token| Some(token.as_str()) == stored_access_token(stored))
        {
            return StoredGrantUse::Reauthorize;
        }
        if let Some(required) = rejected
            .challenge
            .as_deref()
            .and_then(|challenge| insufficient_scope_requirement(challenge, mcp_server_url))
        {
            let granted = scope_set(&stored.granted_scopes);
            if required.is_empty()
                || !required
                    .iter()
                    .all(|scope| granted.contains(scope.as_str()))
            {
                return StoredGrantUse::Reauthorize;
            }
        }
    }

    if access_token_needs_refresh(stored) {
        StoredGrantUse::Refresh
    } else {
        StoredGrantUse::AsIs
    }
}

/// Whether a refresh failed because the authorization server is unreachable or
/// failing, rather than refusing this client or request.
///
/// rmcp reports every failure except `invalid_grant` as `TokenRefreshFailed`
/// with oauth2's message, which drops the HTTP status. A deleted client
/// (`invalid_client`) is among them and only a new authorization recovers it,
/// so only messages known to mean a server failure count; anything else keeps
/// re-authorizing.
fn is_transient_refresh_failure(error: &AuthError) -> bool {
    const SERVER_FAILURES: [&str; 5] = [
        "Request failed",
        "Failed to parse server response",
        "Other error: server returned empty error response",
        "Server returned error response: temporarily_unavailable",
        "Server returned error response: server_error",
    ];
    let AuthError::TokenRefreshFailed(message) = error else {
        return false;
    };
    SERVER_FAILURES
        .iter()
        .any(|failure| message.starts_with(failure))
}

/// Builds a manager over the stored grant, refreshing it when needed. Returns
/// `None` when the grant cannot be used and a new authorization is required,
/// and an error when the authorization server is failing.
async fn reuse_stored_grant(
    mcp_server_url: &str,
    name: &str,
    static_client: Option<&StaticOAuthClientConfig>,
    credentials: &GooseCredentialStore,
    stored: &StoredCredentials,
    stored_use: StoredGrantUse,
) -> Result<Option<AuthorizationManager>, anyhow::Error> {
    if stored_use == StoredGrantUse::Reauthorize {
        return Ok(None);
    }

    let mut auth_manager = AuthorizationManager::new(mcp_server_url).await?;
    auth_manager.set_credential_store(credentials.clone());
    if !auth_manager.initialize_from_store().await? {
        return Ok(None);
    }
    // initialize_from_store configures the client from the stored client_id
    // alone; a confidential client must present its secret at the token
    // endpoint for the refresh to succeed.
    configure_static_client(&mut auth_manager, static_client, mcp_server_url)?;
    if stored_use == StoredGrantUse::AsIs {
        return Ok(Some(auth_manager));
    }

    let mut token_response = match auth_manager.refresh_token().await {
        Ok(token_response) => token_response,
        // Keep the credentials and fail this attempt: a browser login could not
        // finish either, since its code exchange uses the same token endpoint.
        Err(e) if is_transient_refresh_failure(&e) => {
            return Err(StoredCredentialsError::AuthorizationServerFailing(Box::new(e)).into());
        }
        Err(e) => {
            warn!("[OAuth:{}] Token refresh failed: {}", name, e);
            return Ok(None);
        }
    };
    if !restore_omitted_scopes(&mut token_response, &stored.granted_scopes) {
        return Ok(Some(auth_manager));
    }

    credentials.replace_token_response(token_response.clone())?;
    let mut oauth_state = OAuthState::new(mcp_server_url, None).await?;
    oauth_state
        .set_credentials(&stored.client_id, token_response)
        .await?;
    let mut restored_manager = oauth_state
        .into_authorization_manager()
        .ok_or_else(|| anyhow::anyhow!("Failed to restore OAuth authorization manager"))?;
    configure_static_client(&mut restored_manager, static_client, mcp_server_url)?;
    restored_manager.set_credential_store(credentials.clone());
    Ok(Some(restored_manager))
}

/// Authorizes an MCP connection. Without `rejected`, only the stored grant is
/// used (refreshed if needed) and no browser is opened. With it, a grant
/// another session stored while this one waited is reused when it answers the
/// refusal; otherwise the browser authorization flow runs. A refresh that fails
/// because the authorization server is failing never opens the browser.
pub async fn oauth_flow(
    mcp_server_url: &str,
    name: &str,
    static_client: Option<&StaticOAuthClientConfig>,
    rejected: Option<RejectedRequest>,
) -> Result<AuthorizationManager, anyhow::Error> {
    let env_client = env_static_oauth_client();
    let static_client = static_client.or(env_client.as_ref());
    let credential_store = GooseCredentialStore::new(name.to_string());
    // One flow per extension at a time, across every goose process: waiters
    // reuse the grant the first one stores instead of each opening a browser.
    // A holder may be waiting on the browser callback, so wait at least as long.
    let _authorization_lock = credential_store
        .lock_authorization(oauth_callback_timeout() + AUTHORIZATION_LOCK_MARGIN)
        .await?;
    let stored_credentials = credential_store.load().await?;
    let previous_requested_scopes = credential_store.load_requested_scopes()?;
    let previously_granted_scopes = stored_credentials
        .as_ref()
        .map(|stored| stored.granted_scopes.clone())
        .unwrap_or_default();

    if let Some(stored) = stored_credentials.as_ref() {
        let stored_use = stored_grant_use(
            stored,
            static_client,
            previous_requested_scopes.as_deref(),
            rejected.as_ref(),
            mcp_server_url,
        );
        if let Some(manager) = reuse_stored_grant(
            mcp_server_url,
            name,
            static_client,
            &credential_store,
            stored,
            stored_use,
        )
        .await?
        {
            return Ok(manager);
        }
    }

    let Some(RejectedRequest { challenge, .. }) = rejected else {
        return Err(StoredCredentialsError::NeedAuthorization.into());
    };

    let (callback_sender, callback_receiver) = oneshot::channel::<String>();
    let app_state = AppState {
        callback_receiver: Arc::new(Mutex::new(Some(callback_sender))),
    };
    let rendered = render_oauth_callback(name);
    let handler = move |Query(params): Query<CallbackParams>, State(state): State<AppState>| {
        let rendered = rendered.clone();
        async move {
            if let Some(sender) = state.callback_receiver.lock().await.take() {
                let query = serde_urlencoded::to_string([
                    ("code", params.code.as_str()),
                    ("state", params.state.as_str()),
                ])
                .unwrap_or_default();
                let issuer = params
                    .iss
                    .as_deref()
                    .map(|iss| format!("&iss={}", urlencoding::encode(iss)))
                    .unwrap_or_default();
                let _ = sender.send(format!("http://callback/oauth_callback?{query}{issuer}"));
            }
            Html(rendered)
        }
    };
    let app = Router::new()
        .route("/oauth_callback", get(handler))
        .with_state(app_state);

    let port = std::env::var("GOOSE_OAUTH_CALLBACK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    let used_addr = listener.local_addr()?;
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("Callback server error: {}", e);
        }
    });

    let mut oauth_state = OAuthState::new(mcp_server_url, None).await?;
    let redirect_uri = format!("http://127.0.0.1:{}/oauth_callback", used_addr.port());
    oauth_state
        .start_authorization(build_authorization_request(
            redirect_uri.clone(),
            static_client,
            challenge,
            mcp_server_url,
            &previously_granted_scopes,
        ))
        .await?;

    let authorization_url = oauth_state.get_authorization_url().await?;
    let callback_url = async {
        if let Some(callback_url) =
            complete_automatic_authorization(authorization_url.as_str(), &redirect_uri).await?
        {
            Ok(callback_url)
        } else {
            announce_authorization_url(name, authorization_url.as_str());
            if let Err(e) = webbrowser::open(authorization_url.as_str()) {
                warn!(
                    "[OAuth:{}] Failed to open browser automatically: {}",
                    name, e
                );
            }
            wait_for_callback(
                callback_receiver,
                oauth_callback_timeout(),
                name,
                authorization_url.as_str(),
            )
            .await
        }
    }
    .await;
    server_handle.abort();
    oauth_state.handle_callback_url(&callback_url?).await?;

    let (client_id, token_response) = oauth_state.get_credentials().await?;
    let mut auth_manager = oauth_state
        .into_authorization_manager()
        .ok_or_else(|| anyhow::anyhow!("Failed to get authorization manager"))?;

    let granted_scopes = match token_response.as_ref().and_then(|tr| tr.scopes()) {
        Some(scopes) => scopes.iter().map(|scope| scope.to_string()).collect(),
        None => auth_manager.get_current_scopes().await,
    };
    credential_store
        .save_new_grant(
            StoredCredentials::new(
                client_id,
                token_response,
                granted_scopes,
                Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_secs())
                        .unwrap_or(0),
                ),
            ),
            static_client.map(|client| client.scopes.clone()),
        )
        .await?;

    auth_manager.set_credential_store(credential_store);

    Ok(auth_manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_oauth_callback_timeout_uses_default_for_missing_or_invalid_values() {
        assert_eq!(
            resolve_oauth_callback_timeout(None),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
        assert_eq!(
            resolve_oauth_callback_timeout(Some("not-a-number")),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
        assert_eq!(
            resolve_oauth_callback_timeout(Some("0")),
            Duration::from_secs(DEFAULT_OAUTH_CALLBACK_TIMEOUT_SECS)
        );
    }

    #[test]
    fn resolve_oauth_callback_timeout_uses_positive_values() {
        assert_eq!(
            resolve_oauth_callback_timeout(Some("42")),
            Duration::from_secs(42)
        );
    }

    #[test]
    fn oauth_callback_escapes_extension_name() {
        let payload = r#"<script>alert("xss")</script>&"#;
        let rendered = render_oauth_callback(payload);

        assert!(!rendered.contains(payload));
        assert!(rendered.contains("&lt;script&gt;"));
        assert!(rendered.contains("&amp;"));
    }

    #[test]
    fn oauth_callback_preserves_plain_extension_name() {
        let rendered = render_oauth_callback("Example MCP");

        assert!(rendered.contains("Example MCP OAuth Success"));
        assert!(rendered.contains(">Example MCP</span>"));
    }

    #[tokio::test]
    async fn wait_for_callback_returns_received_callback_url() {
        let (sender, receiver) = oneshot::channel();
        let expected = "http://callback/oauth_callback?code=auth-code&state=csrf-state";
        sender.send(expected.to_string()).unwrap();

        let callback_url = wait_for_callback(
            receiver,
            Duration::from_secs(1),
            "test-server",
            "https://auth.example/authorize",
        )
        .await
        .unwrap();

        assert_eq!(callback_url, expected);
    }

    #[test]
    fn callback_params_capture_rfc_9207_issuer() {
        let uri: axum::http::Uri =
            "http://127.0.0.1/oauth_callback?code=auth-code&state=csrf-state&iss=https%3A%2F%2Fauth.example%2Fidp"
                .parse()
                .unwrap();

        let Query(params) = Query::<CallbackParams>::try_from_uri(&uri).unwrap();

        assert_eq!(params.iss.as_deref(), Some("https://auth.example/idp"));
    }

    #[test]
    fn callback_params_accept_missing_issuer() {
        let uri: axum::http::Uri =
            "http://127.0.0.1/oauth_callback?code=auth-code&state=csrf-state"
                .parse()
                .unwrap();

        let Query(params) = Query::<CallbackParams>::try_from_uri(&uri).unwrap();

        assert_eq!(params.iss, None);
    }

    #[test]
    fn unchanged_scope_request_preserves_a_narrowed_grant() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: None,
            scopes: vec!["scope.read".to_string(), "scope.write".to_string()],
        };

        assert!(!configured_scopes_changed(
            Some(&static_client),
            Some(&["scope.read".to_string(), "scope.write".to_string()]),
            &["scope.read".to_string()],
        ));
    }

    #[test]
    fn changed_scope_request_requires_reauthorization() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: None,
            scopes: vec!["scope.read".to_string(), "scope.write".to_string()],
        };

        assert!(configured_scopes_changed(
            Some(&static_client),
            Some(&["scope.read".to_string()]),
            &["scope.read".to_string()],
        ));
    }

    #[test]
    fn changed_static_client_requires_reauthorization() {
        let static_client = StaticOAuthClientConfig {
            client_id: "new-client".to_string(),
            client_secret: None,
            scopes: vec![],
        };

        assert!(configured_client_changed(
            Some(&static_client),
            "old-client"
        ));
        assert!(!configured_client_changed(
            Some(&static_client),
            "new-client"
        ));
        assert!(!configured_client_changed(None, "old-client"));
    }

    #[test]
    fn legacy_grant_reauthorizes_once_only_when_scopes_are_missing() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: None,
            scopes: vec!["scope.read".to_string(), "scope.write".to_string()],
        };

        assert!(configured_scopes_changed(
            Some(&static_client),
            None,
            &["scope.read".to_string()],
        ));
        assert!(!configured_scopes_changed(
            Some(&static_client),
            None,
            &["scope.read".to_string(), "scope.write".to_string()],
        ));
    }

    #[test]
    fn removing_static_client_configuration_requires_reauthorization() {
        assert!(configured_scopes_changed(
            None,
            Some(&["scope.read".to_string()]),
            &["scope.read".to_string()],
        ));
        assert!(!configured_scopes_changed(
            None,
            None,
            &["scope.read".to_string()],
        ));
    }

    #[test]
    fn omitted_refresh_scope_preserves_the_previous_grant() {
        use oauth2::{basic::BasicTokenType, AccessToken};
        use rmcp::transport::auth::VendorExtraTokenFields;

        let previous = vec!["scope.read".to_string()];
        let mut token_response = OAuthTokenResponse::new(
            AccessToken::new("access-token".to_string()),
            BasicTokenType::Bearer,
            VendorExtraTokenFields::default(),
        );

        assert!(restore_omitted_scopes(&mut token_response, &previous));
        assert_eq!(
            token_response
                .scopes()
                .unwrap()
                .iter()
                .map(|scope| scope.as_str())
                .collect::<Vec<_>>(),
            vec!["scope.read"]
        );
        assert!(!restore_omitted_scopes(&mut token_response, &previous));
    }

    #[test]
    fn authorization_request_uses_client_metadata_url_without_static_client() {
        let request = build_authorization_request(
            "http://127.0.0.1:1234/oauth_callback".to_string(),
            None,
            None,
            "https://mcp.example",
            &[],
        );

        assert_eq!(request.client_id, None);
        assert_eq!(request.client_secret, None);
        assert_eq!(
            request.client_metadata_url.as_deref(),
            Some(CLIENT_METADATA_URL)
        );
        assert!(request.scopes.is_empty());
    }

    #[test]
    fn authorization_request_prefers_static_client_over_client_metadata_url() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: Some("registered-secret".to_string()),
            scopes: vec!["scope.read".to_string(), "scope.write".to_string()],
        };

        let request = build_authorization_request(
            "http://127.0.0.1:1234/oauth_callback".to_string(),
            Some(&static_client),
            None,
            "https://mcp.example",
            &[],
        );

        assert_eq!(request.client_id.as_deref(), Some("registered-client"));
        assert_eq!(request.client_secret.as_deref(), Some("registered-secret"));
        assert_eq!(request.client_metadata_url, None);
        assert_eq!(request.scopes, vec!["scope.read", "scope.write"]);
    }

    #[test]
    fn authorization_request_omits_secret_and_scopes_for_public_static_client() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: None,
            scopes: vec![],
        };

        let request = build_authorization_request(
            "http://127.0.0.1:1234/oauth_callback".to_string(),
            Some(&static_client),
            None,
            "https://mcp.example",
            &[],
        );

        assert_eq!(request.client_id.as_deref(), Some("registered-client"));
        assert_eq!(request.client_secret, None);
        assert_eq!(request.client_metadata_url, None);
        assert!(request.scopes.is_empty());
    }

    #[test]
    fn challenge_request_asks_for_the_union_of_granted_and_challenged_scopes() {
        let request = build_authorization_request(
            "http://127.0.0.1:1234/oauth_callback".to_string(),
            None,
            Some(
                r#"Bearer error="insufficient_scope", scope="scope.write scope.admin""#.to_string(),
            ),
            "https://mcp.example",
            &["scope.read".to_string(), "scope.write".to_string()],
        );

        assert_eq!(
            request.scopes,
            vec!["scope.read", "scope.write", "scope.admin"]
        );
        assert!(request.challenge.is_some());
    }

    #[test]
    fn challenge_request_keeps_static_client_scopes() {
        let static_client = StaticOAuthClientConfig {
            client_id: "registered-client".to_string(),
            client_secret: None,
            scopes: vec!["scope.read".to_string()],
        };

        let request = build_authorization_request(
            "http://127.0.0.1:1234/oauth_callback".to_string(),
            Some(&static_client),
            Some(r#"Bearer error="insufficient_scope", scope="scope.write""#.to_string()),
            "https://mcp.example",
            &[],
        );

        assert_eq!(request.client_id.as_deref(), Some("registered-client"));
        assert_eq!(request.scopes, vec!["scope.read", "scope.write"]);
    }

    #[test]
    fn long_lived_token_without_refresh_token_is_not_refreshed() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let token_response: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": "mcp_long_lived",
            "token_type": "bearer",
            "expires_in": 31_536_000,
        }))
        .unwrap();
        let stored = StoredCredentials::new(
            "client-id".to_string(),
            Some(token_response.clone()),
            vec![],
            Some(now),
        );

        assert!(token_response.refresh_token().is_none());
        assert!(
            !access_token_needs_refresh(&stored),
            "a token valid for another year must be used as-is instead of forcing browser re-auth"
        );

        let expired = StoredCredentials::new(
            "client-id".to_string(),
            Some(token_response),
            vec![],
            Some(now - 31_536_000),
        );
        assert!(
            access_token_needs_refresh(&expired),
            "an expired token must still take the refresh path"
        );
    }

    fn stored_grant(
        access_token: &str,
        granted_scopes: &[&str],
        received_at: u64,
    ) -> StoredCredentials {
        let token_response: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": access_token,
            "refresh_token": "refresh",
            "token_type": "bearer",
            "expires_in": 3600,
        }))
        .unwrap();
        StoredCredentials::new(
            "client-id".to_string(),
            Some(token_response),
            granted_scopes
                .iter()
                .map(|scope| scope.to_string())
                .collect(),
            Some(received_at),
        )
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn refused(challenge: Option<&str>, refused_tokens: &[&str]) -> RejectedRequest {
        RejectedRequest {
            challenge: challenge.map(str::to_string),
            refused_tokens: refused_tokens
                .iter()
                .map(|token| token.to_string())
                .collect(),
        }
    }

    fn grant_use(stored: &StoredCredentials, rejected: Option<&RejectedRequest>) -> StoredGrantUse {
        stored_grant_use(stored, None, None, rejected, "https://mcp.example")
    }

    #[test]
    fn stored_grant_is_used_as_is_until_it_needs_a_refresh() {
        assert_eq!(
            grant_use(&stored_grant("t1", &[], now()), None),
            StoredGrantUse::AsIs
        );
        assert_eq!(
            grant_use(&stored_grant("t1", &[], now() - 3600), None),
            StoredGrantUse::Refresh
        );
    }

    #[test]
    fn refused_token_still_in_the_store_needs_a_new_authorization() {
        let stored = stored_grant("t1", &[], now());
        let rejected = refused(Some(r#"Bearer error="invalid_token""#), &["t1"]);

        assert_eq!(
            grant_use(&stored, Some(&rejected)),
            StoredGrantUse::Reauthorize
        );
    }

    #[test]
    fn grant_saved_by_another_session_answers_a_refused_token() {
        let stored = stored_grant("t2", &[], now());
        let rejected = refused(Some(r#"Bearer error="invalid_token""#), &["t1"]);

        assert_eq!(grant_use(&stored, Some(&rejected)), StoredGrantUse::AsIs);
    }

    #[test]
    fn stored_token_refused_by_an_earlier_request_needs_a_new_authorization() {
        let stored = stored_grant("t2", &[], now());
        let rejected = refused(Some(r#"Bearer error="invalid_token""#), &["t2", "t1"]);

        assert_eq!(
            grant_use(&stored, Some(&rejected)),
            StoredGrantUse::Reauthorize
        );
    }

    #[test]
    fn grant_saved_while_waiting_answers_an_unauthenticated_request() {
        let stored = stored_grant("t2", &[], now());
        let rejected = refused(Some(r#"Bearer resource_metadata="/meta""#), &[]);

        assert_eq!(grant_use(&stored, Some(&rejected)), StoredGrantUse::AsIs);
    }

    #[test]
    fn insufficient_scope_reuses_a_stored_grant_only_when_it_covers_the_scope() {
        let rejected = refused(
            Some(r#"Bearer error="insufficient_scope", scope="scope.write""#),
            &["t1"],
        );
        let covering = stored_grant("t2", &["scope.read", "scope.write"], now());
        let narrower = stored_grant("t2", &["scope.read"], now());
        let unscoped = refused(Some(r#"Bearer error="insufficient_scope""#), &["t1"]);

        assert_eq!(grant_use(&covering, Some(&rejected)), StoredGrantUse::AsIs);
        assert_eq!(
            grant_use(&narrower, Some(&rejected)),
            StoredGrantUse::Reauthorize
        );
        assert_eq!(
            grant_use(&covering, Some(&unscoped)),
            StoredGrantUse::Reauthorize
        );
    }

    #[test]
    fn changed_client_configuration_needs_a_new_authorization() {
        let static_client = StaticOAuthClientConfig {
            client_id: "other-client".to_string(),
            client_secret: None,
            scopes: vec![],
        };

        assert_eq!(
            stored_grant_use(
                &stored_grant("t1", &[], now()),
                Some(&static_client),
                None,
                None,
                "https://mcp.example",
            ),
            StoredGrantUse::Reauthorize
        );
    }

    async fn refresh_error_from(authorization_server: &str) -> AuthError {
        use rmcp::transport::auth::{AuthorizationMetadata, InMemoryCredentialStore};

        let store = InMemoryCredentialStore::new();
        store
            .save(stored_grant("access", &[], now()))
            .await
            .unwrap();
        let mut manager = AuthorizationManager::new(authorization_server)
            .await
            .unwrap();
        manager.with_client(reqwest::Client::new()).unwrap();
        let mut metadata = AuthorizationMetadata::default();
        metadata.authorization_endpoint = format!("{authorization_server}/authorize");
        metadata.token_endpoint = format!("{authorization_server}/token");
        manager.set_metadata(metadata);
        manager.configure_client_id("client-id").unwrap();
        manager.set_credential_store(store);
        manager.refresh_token().await.unwrap_err()
    }

    // Runs the real rmcp + oauth2 stack, so a change in how either words these
    // failures fails here instead of silently changing which ones re-authorize.
    #[tokio::test]
    async fn only_a_failing_authorization_server_makes_a_refresh_failure_transient() {
        use serde_json::json;
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

        let cases = [
            (
                "unavailable",
                ResponseTemplate::new(503)
                    .set_body_json(json!({ "error": "temporarily_unavailable" })),
                true,
            ),
            (
                "server error",
                ResponseTemplate::new(500)
                    .set_body_json(json!({ "error": "server_error", "error_description": "down" })),
                true,
            ),
            (
                "proxy error page",
                ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"),
                true,
            ),
            ("empty error body", ResponseTemplate::new(503), true),
            (
                "deleted client",
                ResponseTemplate::new(401).set_body_json(json!({ "error": "invalid_client" })),
                false,
            ),
            (
                "unsupported scope",
                ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_scope" })),
                false,
            ),
            (
                "revoked grant",
                ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
                false,
            ),
        ];
        for (case, response, transient) in cases {
            let server = MockServer::start().await;
            Mock::given(path("/token"))
                .respond_with(response)
                .mount(&server)
                .await;
            let error = refresh_error_from(&server.uri()).await;
            assert_eq!(
                is_transient_refresh_failure(&error),
                transient,
                "{case}: {error}"
            );
        }

        let unreachable = refresh_error_from("http://127.0.0.1:1").await;
        assert!(is_transient_refresh_failure(&unreachable), "{unreachable}");
    }

    #[tokio::test]
    async fn wait_for_callback_times_out_with_authorization_url() {
        let (_sender, receiver) = oneshot::channel();

        let error = wait_for_callback(
            receiver,
            Duration::from_millis(1),
            "test-server",
            "https://auth.example/authorize",
        )
        .await
        .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("test-server"));
        assert!(message.contains("timed out"));
        assert!(message.contains("https://auth.example/authorize"));
    }
}
