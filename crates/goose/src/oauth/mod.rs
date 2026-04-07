mod persist;

pub use persist::has_stored_credentials;

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use minijinja::render;
use oauth2::TokenResponse;
use rmcp::transport::auth::{CredentialStore, OAuthState, StoredCredentials};
use rmcp::transport::AuthorizationManager;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info, warn};

use crate::oauth::persist::GooseCredentialStore;

const CALLBACK_TEMPLATE: &str = include_str!("oauth_callback.html");

#[derive(Clone)]
struct AppState {
    code_receiver: Arc<Mutex<Option<oneshot::Sender<CallbackParams>>>>,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

pub async fn oauth_flow(
    mcp_server_url: &String,
    name: &String,
) -> Result<AuthorizationManager, anyhow::Error> {
    let credential_store = GooseCredentialStore::new(name.clone());
    let mut auth_manager = AuthorizationManager::new(mcp_server_url).await?;
    auth_manager.set_credential_store(credential_store.clone());

    if auth_manager.initialize_from_store().await? {
        info!(
            "[OAuth:{}] Found stored credentials, attempting token refresh",
            name
        );

        match auth_manager.refresh_token().await {
            Ok(token_response) => {
                let has_refresh = token_response.refresh_token().is_some();
                let expires_in = token_response.expires_in();
                info!(
                    "[OAuth:{}] Token refresh succeeded - has_refresh_token: {}, expires_in: {:?}",
                    name, has_refresh, expires_in
                );
                return Ok(auth_manager);
            }
            Err(e) => {
                warn!(
                    "[OAuth:{}] Token refresh failed: {} - clearing stored credentials and falling back to browser auth",
                    name, e
                );
            }
        }

        if let Err(e) = credential_store.clear().await {
            warn!("[OAuth:{}] error clearing bad credentials: {}", name, e);
        }
    } else {
        info!(
            "[OAuth:{}] No stored credentials found, starting browser OAuth flow",
            name
        );
    }

    // No existing credentials or they were invalid - need to do the full oauth flow
    let (code_sender, code_receiver) = oneshot::channel::<CallbackParams>();
    let app_state = AppState {
        code_receiver: Arc::new(Mutex::new(Some(code_sender))),
    };

    let rendered = render!(CALLBACK_TEMPLATE, name => name);
    let handler = move |Query(params): Query<CallbackParams>, State(state): State<AppState>| {
        let rendered = rendered.clone();
        async move {
            if let Some(sender) = state.code_receiver.lock().await.take() {
                let _ = sender.send(params);
            }
            Html(rendered)
        }
    };
    let app = Router::new()
        .route("/oauth_callback", get(handler))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let used_addr = listener.local_addr()?;
    tokio::spawn(async move {
        let result = axum::serve(listener, app).await;
        if let Err(e) = result {
            eprintln!("Callback server error: {}", e);
        }
    });

    let mut oauth_state = OAuthState::new(mcp_server_url, None).await?;

    let redirect_uri = format!("http://localhost:{}/oauth_callback", used_addr.port());
    oauth_state
        .start_authorization(&[], redirect_uri.as_str(), Some("goose"))
        .await?;

    let authorization_url = oauth_state.get_authorization_url().await?;
    if webbrowser::open(authorization_url.as_str()).is_err() {
        eprintln!("Open the following URL to authorize {}:", name);
        eprintln!("  {}", authorization_url);
    }

    let CallbackParams {
        code: auth_code,
        state: csrf_token,
    } = code_receiver.await?;
    oauth_state.handle_callback(&auth_code, &csrf_token).await?;

    let (client_id, token_response) = oauth_state.get_credentials().await?;

    let has_refresh_token = token_response
        .as_ref()
        .and_then(|tr| tr.refresh_token())
        .is_some();
    let expires_in = token_response.as_ref().and_then(|tr| tr.expires_in());
    let scopes: Vec<String> = token_response
        .as_ref()
        .and_then(|tr| tr.scopes())
        .map(|s| s.iter().map(|sc| sc.to_string()).collect())
        .unwrap_or_default();

    debug!(
        "[OAuth:{}] Browser auth completed - has_refresh_token: {}, expires_in: {:?}, scopes: {:?}",
        name, has_refresh_token, expires_in, scopes
    );

    let mut auth_manager = oauth_state
        .into_authorization_manager()
        .ok_or_else(|| anyhow::anyhow!("Failed to get authorization manager"))?;

    let granted_scopes: Vec<String> = token_response
        .as_ref()
        .and_then(|tr| tr.scopes())
        .map(|scopes| scopes.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    credential_store
        .save(StoredCredentials {
            client_id,
            token_response,
            granted_scopes,
            token_received_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0),
            ),
        })
        .await?;

    auth_manager.set_credential_store(credential_store);

    Ok(auth_manager)
}
