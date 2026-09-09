use clap::Parser;
use photos_storage_node::{backend::LocalDisk, grant::Unsigned, router, AppState};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

/// photos storage node: stores encrypted, content-addressed blobs.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Address to listen on. Bind to your Tailscale IP to expose it only to your tailnet.
    #[arg(long, env = "PHOTOS_NODE_LISTEN", default_value = "127.0.0.1:4100")]
    listen: SocketAddr,

    /// Grants are not signature-checked yet, so anyone who can reach the node
    /// can write and delete. Pass this to acknowledge that when binding to a
    /// non-loopback address (e.g. a Tailscale IP on a trusted tailnet).
    #[arg(long, env = "PHOTOS_NODE_ALLOW_UNSIGNED_GRANTS")]
    allow_unsigned_grants: bool,

    /// Directory for blob storage (LocalDisk backend).
    #[arg(long, env = "PHOTOS_NODE_DATA_DIR", default_value = "./data/blobs")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    if !args.listen.ip().is_loopback() {
        anyhow::ensure!(
            args.allow_unsigned_grants,
            "refusing to bind {} without --allow-unsigned-grants: grant signatures are not \
             verified yet, so any peer could write or delete blobs",
            args.listen
        );
        tracing::warn!(addr = %args.listen, "serving unsigned grants on a non-loopback address");
    }
    let backend = LocalDisk::open(&args.data_dir).await?;
    tracing::info!(dir = %args.data_dir.display(), "LocalDisk backend ready");

    let state = AppState { backend: Arc::new(backend), grants: Arc::new(Unsigned::default()) };
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(addr = %args.listen, "storage node listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
