use std::env;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;

use super::{AnalyticsEvent, CopilotAnalytics, CopilotReposResponse, RoutingPrefs};

pub const SWITCHBOARD_URL_ENV: &str = "GOOSE_COPILOT_SWITCHBOARD_URL";
pub const INSTALLATION_ID_CONFIG_KEY: &str = "copilot_installation_id";

pub fn switchboard_url() -> Result<String> {
    env::var(SWITCHBOARD_URL_ENV).map_err(|_| {
        anyhow!(
            "{SWITCHBOARD_URL_ENV} must be set to your Cloudflare switchboard URL (see ui/desktop/.env.example)"
        )
    })
}

pub fn extract_agent_id(tunnel_url: &str) -> Option<String> {
    tunnel_url
        .rsplit_once("/tunnel/")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("").to_string())
}

pub async fn fetch_oauth_client_id() -> Result<String> {
    #[derive(Deserialize)]
    struct OAuthConfig {
        oauth_client_id: String,
    }
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .get(format!("{switchboard}/copilot/oauth-config"))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("switchboard returned {}", res.status());
    }
    let cfg: OAuthConfig = res.json().await?;
    if cfg.oauth_client_id.trim().is_empty() {
        bail!("switchboard returned an empty oauth_client_id");
    }
    Ok(cfg.oauth_client_id)
}

#[derive(Debug, Clone)]
pub struct TunnelSnapshot {
    pub url: String,
    pub secret: String,
}

#[derive(Debug, Clone)]
pub struct InstallCredentials {
    pub installation_id: u64,
    pub tunnel_secret: String,
}

pub async fn resolve_install_credentials(tunnel: TunnelSnapshot) -> Result<InstallCredentials> {
    let tunnel_secret = if tunnel.secret.is_empty() {
        Config::global()
            .get_secret::<String>("tunnel_secret")
            .context("tunnel_secret unavailable; complete setup first")?
    } else {
        tunnel.secret
    };
    let agent_id = extract_agent_id(&tunnel.url)
        .context("tunnel URL is missing the agent id; complete setup first")?;

    if let Ok(cached) = Config::global().get_param::<u64>(INSTALLATION_ID_CONFIG_KEY) {
        return Ok(InstallCredentials {
            installation_id: cached,
            tunnel_secret,
        });
    }

    let resolved = resolve_install_id(&agent_id, &tunnel_secret).await?;
    let _ = Config::global().set_param(INSTALLATION_ID_CONFIG_KEY, serde_json::json!(resolved));
    Ok(InstallCredentials {
        installation_id: resolved,
        tunnel_secret,
    })
}

async fn resolve_install_id(agent_id: &str, tunnel_secret: &str) -> Result<u64> {
    #[derive(Deserialize)]
    struct WhoamiResponse {
        installation_id: u64,
    }
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .post(format!("{switchboard}/copilot/whoami"))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "tunnel_secret": tunnel_secret,
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard whoami rejected: {status} {detail}");
    }
    let body: WhoamiResponse = res.json().await?;
    Ok(body.installation_id)
}

#[derive(Debug)]
pub struct RegisterInstallRequest {
    pub oauth_code: String,
    pub agent_id: String,
    pub tunnel_secret: String,
    pub tunnel_url: String,
}

pub async fn register_installation(req: RegisterInstallRequest) -> Result<u64> {
    #[derive(Deserialize)]
    struct RegisterResponse {
        installation_id: u64,
    }
    let switchboard = switchboard_url()?;
    let body = serde_json::json!({
        "oauth_code": req.oauth_code,
        "agent_id": req.agent_id,
        "tunnel_secret": req.tunnel_secret,
        "tunnel_url": req.tunnel_url,
    });
    let res = Client::new()
        .post(format!("{switchboard}/copilot/register"))
        .json(&body)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard rejected registration: {status} {detail}");
    }
    let register: RegisterResponse = res.json().await?;
    Ok(register.installation_id)
}

pub async fn forward_routing_prefs(
    creds: &InstallCredentials,
    routing: &RoutingPrefs,
) -> Result<()> {
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .put(format!("{switchboard}/copilot/routing-prefs"))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .json(routing)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard rejected routing prefs: {status} {detail}");
    }
    Ok(())
}

pub async fn fetch_repos(creds: &InstallCredentials) -> Result<CopilotReposResponse> {
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .get(format!("{switchboard}/copilot/repos"))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard returned {status}: {detail}");
    }
    res.json().await.map_err(Into::into)
}

pub async fn fetch_analytics(creds: &InstallCredentials) -> Result<CopilotAnalytics> {
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .get(format!("{switchboard}/copilot/analytics"))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard returned {status}: {detail}");
    }
    res.json().await.map_err(Into::into)
}

pub async fn unregister_installation(creds: &InstallCredentials) -> Result<()> {
    let switchboard = switchboard_url()?;
    let res = Client::new()
        .delete(format!("{switchboard}/copilot/install"))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .send()
        .await?;
    if !res.status().is_success() {
        let status = res.status();
        let detail = res.text().await.unwrap_or_default();
        bail!("switchboard rejected unregister: {status} {detail}");
    }
    Ok(())
}

pub async fn disconnect_install(tunnel: TunnelSnapshot) -> Result<()> {
    if let Ok(creds) = resolve_install_credentials(tunnel).await {
        let _ = unregister_installation(&creds).await;
    }
    super::store::clear_install(Config::global());
    Ok(())
}

pub async fn report_analytics_event(tunnel: TunnelSnapshot, event: AnalyticsEvent) {
    let Ok(creds) = resolve_install_credentials(tunnel).await else {
        return;
    };
    let Ok(switchboard) = switchboard_url() else {
        return;
    };
    let _ = Client::new()
        .post(format!("{switchboard}/copilot/analytics/event"))
        .header("X-Install-Id", creds.installation_id.to_string())
        .header("X-Install-Secret", &creds.tunnel_secret)
        .json(&event)
        .send()
        .await;
}
