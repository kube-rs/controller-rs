#![allow(unused_imports, unused_variables)]
pub use controller::*;
use kube::core::response::StatusCause;
use prometheus::{Encoder, TextEncoder};
use std::{
    convert::{Infallible, TryFrom},
    net::SocketAddr,
};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{debug, error, info, instrument, trace, warn};
use tracing_subscriber::{prelude::*, EnvFilter, Registry};

use axum::{
    body::{Body, Bytes, Full},
    extract::{Extension, Path},
    handler::get,
    http::{Response, StatusCode},
    response::IntoResponse,
    AddExtensionLayer, Json, Router,
};
use tower_http::trace::TraceLayer;

// Intended route: /metrics
#[instrument(skip(c))]
async fn metrics(c: Extension<Manager>) -> (StatusCode, Vec<u8>) {
    let metrics = c.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    (StatusCode::OK, buffer)
}

// Intended route: /health
#[instrument]
async fn health() -> (StatusCode, Json<&'static str>) {
    (StatusCode::OK, Json("healthy"))
}

// Intended route: /
#[instrument(skip(c))]
async fn index(c: Extension<Manager>) -> Result<Json<controller::manager::State>, Infallible> {
    let state = c.state().await;
    Ok(Json(state))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing layers
    #[cfg(feature = "telemetry")]
    let telemetry = tracing_opentelemetry::layer().with_tracer(telemetry::init_tracer().await);
    let logger = tracing_subscriber::fmt::layer().json();
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    // Decide on layers
    #[cfg(feature = "telemetry")]
    let collector = Registry::default().with(telemetry).with(logger).with(env_filter);
    #[cfg(not(feature = "telemetry"))]
    let collector = Registry::default().with(logger).with(env_filter);

    // Initialize tracing
    tracing::subscriber::set_global_default(collector).unwrap();

    // Start kubernetes controller
    let (manager, drainer) = Manager::new().await;

    // Define routes
    let app = Router::new()
        .route("/", get(index))
        .route("/metrics", get(metrics))
        .layer(AddExtensionLayer::new(manager.clone()))
        .layer(TraceLayer::new_for_http())
        .boxed()
        // Reminder: routes added *after* TraceLayer are not subject to its logging behavior
        .route("/health", get(health));

    // Start web server
    let mut shutdown = signal(SignalKind::terminate()).expect("could not monitor for SIGTERM");
    let server = axum::Server::bind(&SocketAddr::from(([0, 0, 0, 0], 8080)))
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            shutdown.recv().await;
        });

    tokio::select! {
        _ = drainer => warn!("controller drained"),
        _ = server => info!("axum exited"),
    }
    Ok(())
}
