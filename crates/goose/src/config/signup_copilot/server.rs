use anyhow::Result;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::sync::oneshot;

use super::InstallCallback;

static TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/config/signup_copilot/templates");

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
struct Inner {
    tx: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<InstallCallback>>>>,
    expected_state: String,
}

pub async fn run_callback_server(
    cb_tx: oneshot::Sender<InstallCallback>,
    shutdown_rx: oneshot::Receiver<()>,
    expected_state: String,
    port: u16,
) -> Result<()> {
    let inner = Inner {
        tx: std::sync::Arc::new(tokio::sync::Mutex::new(Some(cb_tx))),
        expected_state,
    };

    let app = Router::new()
        .route("/", get(handle_callback))
        .with_state(inner);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .await?;

    Ok(())
}

async fn handle_callback(
    Query(params): Query<CallbackQuery>,
    axum::extract::State(inner): axum::extract::State<Inner>,
) -> impl IntoResponse {
    if let Some(error) = params.error {
        return render(
            &TEMPLATES_DIR,
            "error.html",
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    let code = match params.code {
        Some(c) => c,
        None => {
            return render(
                &TEMPLATES_DIR,
                "error.html",
                StatusCode::BAD_REQUEST,
                "missing oauth code",
            )
        }
    };
    let state = params.state.unwrap_or_default();
    if state != inner.expected_state {
        return render(
            &TEMPLATES_DIR,
            "error.html",
            StatusCode::BAD_REQUEST,
            "state mismatch",
        );
    }
    let mut tx_guard = inner.tx.lock().await;
    if let Some(tx) = tx_guard.take() {
        let _ = tx.send(InstallCallback { oauth_code: code });
    }

    render(&TEMPLATES_DIR, "success.html", StatusCode::OK, "")
}

fn render(dir: &Dir, name: &str, status: StatusCode, message: &str) -> (StatusCode, Html<String>) {
    let body = dir
        .get_file(name)
        .and_then(|f| f.contents_utf8())
        .map(|s| s.replace("{{ message }}", message))
        .unwrap_or_else(|| format!("<h1>Goose Copilot</h1><p>{}</p>", message));
    (status, Html(body))
}
