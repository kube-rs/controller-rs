#![allow(unused_imports, unused_variables)]
use std::env;
use log::{info, warn, error, debug, trace};
pub use operator::*;

use actix_web::{
  web::{self, Data},
  App, HttpServer, HttpRequest, HttpResponse, Responder, middleware,
};

fn index(state: Data<State>, req: HttpRequest) -> HttpResponse {
    let foos = state.foos().unwrap().data;
    HttpResponse::Ok().json(foos)
}

fn health(_: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json("healthy")
}

fn main() {
    // Logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "actix_web=info,operator=info,kubernetes=info");
        //env::set_var("RUST_LOG", "actix_web=info,operator=debug,kubernetes=debug");
    }
    env_logger::init();

    // Set up kube access + fetch initial state. Crashing on failure here.
    let cfg = match env::var("HOME").expect("have HOME dir").as_ref() {
        "/root" => kubernetes::config::incluster_config(),
        _ => kubernetes::config::load_kube_config(),
    }.expect("Failed to load kube config");
    let shared_state = state::init(cfg).unwrap();

    // Web server
    let sys = actix::System::new("operator");
    HttpServer::new(move || {
        App::new()
            .data(shared_state.clone())
            .wrap(middleware::Logger::default()
                .exclude("/health")
            )
            .service(web::resource("/health").to(health))
            .service(web::resource("/").to(index))
        })
        .bind("0.0.0.0:8080").expect("Can not bind to 0.0.0.0:8080")
        .shutdown_timeout(0) // example server
        .start();
    info!("Starting listening on 0.0.0.0:8080");
    let _ = sys.run();
}
