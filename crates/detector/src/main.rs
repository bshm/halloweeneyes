use anyhow::Result;
use detector::{capture, server};
use shared::EyeState;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("detector=info".parse()?),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: detector <bind-addr> [rtsp-url]");
        eprintln!("  with camera:    detector 0.0.0.0:8080 rtsp://camera/live");
        eprintln!("  simulation only: detector 0.0.0.0:8080");
        std::process::exit(1);
    }

    let bind_addr = args[1].clone();
    let rtsp_url: Option<String> = args.get(2).cloned();

    info!("detector starting");
    if rtsp_url.is_none() {
        info!("no rtsp-url given — simulation-only mode");
    }

    let (tx, _rx) = broadcast::channel::<EyeState>(64);
    let tx = Arc::new(tx);

    let capture_tx = tx.clone();

    let capture_handle = tokio::spawn(async move {
        capture::run(rtsp_url, capture_tx).await
    });

    let server_handle = tokio::spawn(server::run(bind_addr, tx));

    tokio::select! {
        res = capture_handle => { res??; }
        res = server_handle  => { res??; }
    }

    Ok(())
}
