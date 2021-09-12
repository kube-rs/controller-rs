#![allow(unused_imports, unused_variables)]
pub use controller::*;
use prometheus::{Encoder, TextEncoder};
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{prelude::*, EnvFilter, Registry};

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

#[cfg(feature = "telemetry")]
async fn init_tracer() -> opentelemetry::sdk::trace::Tracer {
    let otlp_endpoint =
        std::env::var("OPENTELEMETRY_ENDPOINT_URL").expect("Need a otel tracing collector configured");

    let channel = tonic::transport::Channel::from_shared(otlp_endpoint)
        .unwrap()
        .connect()
        .await
        .unwrap();

    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_channel(channel))
        .with_trace_config(opentelemetry::sdk::trace::config().with_resource(
            opentelemetry::sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                "foo-controller",
            )]),
        ))
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap()
}

#[actix_rt::main]
async fn main() -> Result<()> {
    // Setup tracing layers
    #[cfg(feature = "telemetry")]
    let telemetry = tracing_opentelemetry::layer().with_tracer(init_tracer().await);
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

    // Start web server
    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(manager.clone()))
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:8080")
    .expect("Can not bind to 0.0.0.0:8080")
    .shutdown_timeout(5);

    tokio::select! {
        _ = drainer => warn!("controller drained"),
        _ = server.run() => info!("actix exited"),
    }
    Ok(())
}
