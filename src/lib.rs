use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Finalizer Error: {0}")]
    FinalizerError(#[source] kube::runtime::finalizer::Error<kube::Error>),

    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// State machinery for kube, as exposeable to actix
pub mod manager;
pub use manager::Manager;

/// Generated type, for crdgen
pub use manager::Document;

/// Log and trace integrations
pub mod telemetry;
