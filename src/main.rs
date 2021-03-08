#![allow(unused_imports, unused_variables)]
pub use controller::*;
use prometheus::{Encoder, TextEncoder};
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::{Registry, fmt::format::FmtSpan};

use actix_web::{
    get, middleware,
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};

#[get("/metrics")]
async fn metrics(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
    let metrics = c.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    HttpResponse::Ok().body(buffer)
}

#[get("/health")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/")]
async fn index(c: Data<Manager>, _req: HttpRequest) -> impl Responder {
    let state = c.state().await;
    HttpResponse::Ok().json(&state)
}

#[actix_rt::main]
async fn main() -> Result<()> {
    let otlp_endpoint = std::env::var("OPENTELEMETRY_ENDPOINT_URL")
        //.unwrap_or("http://grafana-agent-traces.monitoring.svc.cluster.local:55680")
        .unwrap_or("http://0.0.0.0:55680".to_string());

    let (tracer, _uninstall) = opentelemetry_otlp::new_pipeline()
        .with_endpoint(&otlp_endpoint)
        .install().unwrap();

    // Finish layers
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = tracing_subscriber::fmt::layer();//.with_span_events(FmtSpan::ACTIVE);

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info")).unwrap();

    // Register all subscribers
    let collector = Registry::default()
        .with(filter_layer)
        .with(telemetry)
        .with(logger);
    tracing::subscriber::set_global_default(collector).unwrap();

    let client = kube::Client::try_default().await.expect("create client");
    let (manager, drainer) = Manager::new(client).await;

    let server = HttpServer::new(move || {
        App::new()
            .data(manager.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:8080")
    .expect("Can not bind to 0.0.0.0:8080")
    .shutdown_timeout(0);

    tokio::select! {
        _ = drainer => warn!("controller drained"),
        _ = server.run() => info!("actix exited"),
    }
    Ok(())
}
