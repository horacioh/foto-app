use clap::Parser;
use photos_storage_node::{backend::LocalDisk, grant::AllowAll, router, AppState};
use std::{path::PathBuf, sync::Arc};

/// photos storage node: stores encrypted, content-addressed blobs.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Address to listen on. Bind to your Tailscale IP to expose it only to your tailnet.
    #[arg(long, env = "PHOTOS_NODE_LISTEN", default_value = "0.0.0.0:4100")]
    listen: String,

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
    let backend = LocalDisk::open(&args.data_dir).await?;
    tracing::info!(dir = %args.data_dir.display(), "LocalDisk backend ready");

    let state = AppState { backend: Arc::new(backend), grants: Arc::new(AllowAll) };
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    tracing::info!(addr = %args.listen, "storage node listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
