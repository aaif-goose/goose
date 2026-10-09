use anyhow::Result;
use goose::agents::StateMachineServices;
use goose::gateway::manager::GatewayManager;
use std::sync::Arc;

pub async fn handle_gateway_status() -> Result<()> {
    let services = StateMachineServices::instance().await;
    let gateway_manager = Arc::new(GatewayManager::new(services)?);
    let statuses = gateway_manager.status().await;

    if statuses.is_empty() {
        println!("No gateways configured.");
        return Ok(());
    }

    for status in statuses {
        let state = if status.running { "running" } else { "stopped" };
        println!(
            "{}: {} ({} paired users)",
            status.gateway_type,
            state,
            status.paired_users.len()
        );
        for user in &status.paired_users {
            println!(
                "  - {}/{} (session: {})",
                user.platform,
                user.display_name.as_deref().unwrap_or(&user.user_id),
                user.session_id
            );
        }
    }

    Ok(())
}

pub async fn handle_gateway_start(
    gateway_type: String,
    platform_config: serde_json::Value,
) -> Result<()> {
    let services = StateMachineServices::instance().await;
    let gateway_manager = Arc::new(GatewayManager::new(services)?);

    let mut config = goose::gateway::GatewayConfig {
        gateway_type,
        platform_config,
        max_sessions: 0,
    };

    let gw = goose::gateway::create_gateway(&mut config)?;
    gateway_manager.start_gateway(config, gw).await?;

    println!("Gateway started. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    gateway_manager.stop_all().await;

    Ok(())
}

pub async fn handle_gateway_stop(gateway_type: String) -> Result<()> {
    let services = StateMachineServices::instance().await;
    let gateway_manager = Arc::new(GatewayManager::new(services)?);
    gateway_manager.stop_gateway(&gateway_type).await?;
    println!("Gateway '{}' stopped.", gateway_type);
    Ok(())
}

pub async fn handle_gateway_pair(gateway_type: String) -> Result<()> {
    let services = StateMachineServices::instance().await;
    let gateway_manager = Arc::new(GatewayManager::new(services)?);
    let (code, expires_at) = gateway_manager.generate_pairing_code(&gateway_type).await?;

    let expires = chrono::DateTime::from_timestamp(expires_at, 0)
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("Pairing code: {}", code);
    println!("Expires at: {}", expires);

    Ok(())
}
