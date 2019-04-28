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
    //sentry::integrations::panic::register_panic_handler();
    //let dsn = env::var("SENTRY_DSN").expect("Sentry DSN required");
    //let _guard = sentry::init(dsn); // must keep _guard in scope

    env::set_var("RUST_LOG", "actix_web=info,operator=info,kubernetes=info");
    if let Ok(level) = env::var("LOG_LEVEL") {
        if level.to_lowercase() == "debug" {
            env::set_var("RUST_LOG", "actix_web=info,operator=debug,kubernetes=info");
        }
    }
    env_logger::init();
    let cfg = match env::var("HOME").expect("have HOME dir").as_ref() {
        "/root" => kubernetes::config::incluster_config(),
        _ => kubernetes::config::load_kube_config(),
    }.expect("Failed to load kube config");
    let shared_state = state::init(cfg).unwrap(); // crash if init fails

    let sys = actix::System::new("raftcat");
    HttpServer::new(move || {
        App::new()
            .data(shared_state.clone())
            .wrap(middleware::Logger::default()
                .exclude("/health")
                .exclude("/favicon.ico")
            )
            //.middleware(sentry_actix::SentryMiddleware::new())
            //.handler("/static", actix_web::fs::StaticFiles::new("./static").unwrap())
            .service(web::resource("/health").to(health)) // TODO: get/vs/post
            .service(web::resource("/").to(index))
        })
        .bind("0.0.0.0:8080").expect("Can not bind to 0.0.0.0:8080")
        .shutdown_timeout(0)    // <- Set shutdown timeout to 0 seconds (default 60s)
        .start();

    info!("Starting listening on 0.0.0.0:8080");
    let _ = sys.run();
    std::process::exit(0); // SIGTERM ends up here eventually
}
