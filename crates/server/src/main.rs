use clap::Parser;
use photos_server::{router, AppState, Config};

/// photos coordinator.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(long, env = "PHOTOS_SERVER_LISTEN", default_value = "0.0.0.0:4000")]
    listen: String,

    /// Postgres connection string. Required from phase 1 onward.
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Public base URL clients and storage nodes use to reach this coordinator.
    #[arg(long, env = "PHOTOS_PUBLIC_URL", default_value = "http://localhost:4000")]
    public_url: String,
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
    let state =
        AppState::new(Config { public_url: args.public_url, database_url: args.database_url });
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    tracing::info!(addr = %args.listen, "coordinator listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
