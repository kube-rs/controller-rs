#![allow(unused_imports, unused_variables)]
use std::env;
use log::{info, warn, error, debug, trace};
use prometheus::{TextEncoder, Encoder};
pub use controller::*;

use actix_web::{
  web::{self, Data},
  App, HttpServer, HttpRequest, HttpResponse, middleware,
};

fn metrics(c: Data<Controller>, _req: HttpRequest) -> HttpResponse {
    let metrics = c.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    HttpResponse::Ok().body(buffer)
}

fn index(c: Data<Controller>, _req: HttpRequest) -> HttpResponse {
    let state = c.state().unwrap();
    HttpResponse::Ok().json(state)
}

fn health(_: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json("healthy")
}

// TODO: tokio main interaction with actix?
#[tokio::main]
async fn main() -> Result<()> {
    // Logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "actix_web=info,controller=info,kube=debug");
    }
    env_logger::init();

    // Set up kube access + fetch initial state. Crashing on failure here.
    let cfg = match kube::config::incluster_config() {
        Ok(c) => c,
        Err(_) => kube::config::load_kube_config().await?,
    };
    let c = state::init(cfg).await.expect("Failed to initialize controller");

    // Web server
    let sys = actix::System::new("controller");
    HttpServer::new(move || {
        App::new()
            .data(c.clone())
            .wrap(middleware::Logger::default()
                .exclude("/health")
            )
            .service(web::resource("/").to(index))
            .service(web::resource("/health").to(health))
            .service(web::resource("/metrics").to(metrics))
        })
        .bind("0.0.0.0:8080").expect("Can not bind to 0.0.0.0:8080")
        .shutdown_timeout(0) // example server
        .start();
    info!("Starting listening on 0.0.0.0:8080");
    let _ = sys.run();
    Ok(())
}
