#![allow(unused_imports, unused_variables)]
pub use controller::*;
use prometheus::{Encoder, TextEncoder};
use tracing::{debug, error, info, trace, warn};

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
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .json()
        .init();
    // TODO: tracing with json logs
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
