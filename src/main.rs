use std::sync::Arc;

use clap::Parser;
use qsbit_studio::{Config, api, jobs::Studio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();
    let address = config.listen;
    let state = Studio::new(config).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("QSBIT Studio: http://{}", listener.local_addr()?);
    let shutdown_state = Arc::clone(&state);
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(async move {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("register SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            shutdown_state.shutdown().await;
        })
        .await?;
    Ok(())
}
