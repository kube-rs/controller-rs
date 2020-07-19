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
async fn metrics(c: Data<Controller>, _req: HttpRequest) -> impl Responder {
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
async fn index(c: Data<Controller>, _req: HttpRequest) -> impl Responder {
    let state = c.state().unwrap();
    HttpResponse::Ok().json(state)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    // Logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "actix_web=info,controller=info,kube=debug");
    }
    env_logger::init();

    // Set up kube access + fetch initial state. Crashing on failure here.
    let client = kube::Client::try_default().await.expect("create client");
    let c = state::init(client)
        .await
        .expect("Failed to initialize controller");

    HttpServer::new(move || {
        App::new()
            .data(c.clone())
            .wrap(middleware::Logger::default().exclude("/health"))
            .service(index)
            .service(health)
            .service(metrics)
    })
    .bind("0.0.0.0:8080")
    .expect("Can not bind to 0.0.0.0:8080")
    .shutdown_timeout(0) // example server
    .start()
    .await
}
