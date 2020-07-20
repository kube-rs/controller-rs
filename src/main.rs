#![allow(unused_imports, unused_variables)]
pub use controller::*;
use log::{debug, error, info, trace, warn};
use prometheus::{Encoder, TextEncoder};
use std::env;

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
    HttpResponse::Ok().json(state)
}

#[actix_rt::main]
async fn main() -> Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "actix_web=info,controller=info,kube=debug");
    }
    env_logger::init();
    let client = kube::Client::try_default().await.expect("create client");
    let manager = Manager::new(client);
    let manager_state = manager.clone();

    let server = HttpServer::new(move || {
        App::new()
            .data(manager_state.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:8080")
    .expect("Can not bind to 0.0.0.0:8080")
    .shutdown_timeout(0);

    tokio::select! {
        _ = manager.run() => warn!("controller drained"),
        _ = server.run() => info!("actix exited"),
    }
    Ok(())
}
