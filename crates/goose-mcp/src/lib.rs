use etcetera::AppStrategyArgs;
use once_cell::sync::Lazy;
#[cfg(any(
    feature = "autovisualiser",
    feature = "computer-controller",
    feature = "memory-server",
    feature = "tutorial-server"
))]
use rmcp::{ServerHandler, ServiceExt};
use std::collections::HashMap;

// NOTE: "Block" is kept here for backwards compatibility with existing
// user config/data directories. Changing this would orphan existing installations.
pub static APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "goose".to_string(),
});

#[cfg(feature = "autovisualiser")]
pub mod autovisualiser;
#[cfg(feature = "computer-controller")]
pub mod computercontroller;
#[cfg(any(
    feature = "autovisualiser",
    feature = "computer-controller",
    feature = "memory-server",
    feature = "tutorial-server"
))]
pub mod mcp_server_runner;
#[cfg(feature = "memory-server")]
mod memory;
#[cfg(all(target_os = "macos", feature = "computer-controller"))]
pub mod peekaboo;
#[cfg(feature = "computer-controller")]
pub mod subprocess;
#[cfg(feature = "tutorial-server")]
pub mod tutorial;

#[cfg(feature = "autovisualiser")]
pub use autovisualiser::AutoVisualiserRouter;
#[cfg(feature = "computer-controller")]
pub use computercontroller::ComputerControllerServer;
#[cfg(feature = "memory-server")]
pub use memory::MemoryServer;
#[cfg(feature = "tutorial-server")]
pub use tutorial::TutorialServer;

pub type SpawnServerFn = fn(tokio::io::DuplexStream, tokio::io::DuplexStream);

#[cfg(any(
    feature = "autovisualiser",
    feature = "computer-controller",
    feature = "memory-server",
    feature = "tutorial-server"
))]
fn spawn_and_serve<S>(
    name: &'static str,
    server: S,
    transport: (tokio::io::DuplexStream, tokio::io::DuplexStream),
) where
    S: ServerHandler + Send + 'static,
{
    tokio::spawn(async move {
        match server.serve(transport).await {
            Ok(running) => {
                let _ = running.waiting().await;
            }
            Err(e) => tracing::error!(builtin = name, error = %e, "server error"),
        }
    });
}

#[cfg(any(
    feature = "autovisualiser",
    feature = "computer-controller",
    feature = "memory-server",
    feature = "tutorial-server"
))]
macro_rules! builtin {
    ($name:ident, $server_ty:ty) => {{
        fn spawn(r: tokio::io::DuplexStream, w: tokio::io::DuplexStream) {
            spawn_and_serve(stringify!($name), <$server_ty>::new(), (r, w));
        }
        (stringify!($name), spawn as SpawnServerFn)
    }};
}

pub static BUILTIN_EXTENSIONS: Lazy<HashMap<&'static str, SpawnServerFn>> = Lazy::new(|| {
    #[allow(unused_mut)]
    let mut extensions = HashMap::new();
    #[cfg(feature = "autovisualiser")]
    extensions.extend([builtin!(autovisualiser, AutoVisualiserRouter)]);
    #[cfg(feature = "computer-controller")]
    extensions.extend([builtin!(computercontroller, ComputerControllerServer)]);
    #[cfg(feature = "memory-server")]
    extensions.extend([builtin!(memory, MemoryServer)]);
    #[cfg(feature = "tutorial-server")]
    extensions.extend([builtin!(tutorial, TutorialServer)]);
    extensions
});
