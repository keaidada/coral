// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// `coral-service` binary — tiny HTTP front-end over coral-core.
//
// Invocation:
//     coral-service                       # bind 0.0.0.0:8080
//     CORAL_BIND=127.0.0.1:9000 coral-service
//     RUST_LOG=coral_service=debug coral-service

use anyhow::Context;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("coral_service=info")),
        )
        .init();

    let bind: SocketAddr = std::env::var("CORAL_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .context("parsing CORAL_BIND")?;

    tracing::info!(%bind, "coral-service listening");
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    axum::serve(listener, coral_service::router())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("received Ctrl-C, shutting down");
}
