use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),

    #[error("IllegalDocument")]
    IllegalDocument,
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Extract the raw enum names involved (recursively) for each nested Error
///
/// This allows us to use the error as a label inside a metric
pub fn error_fmt(e: impl std::error::Error) -> String {
    let mut src = vec![fmt_dbg_error(&e)];
    let mut current = e.source();
    while let Some(cause) = current {
        src.push(fmt_dbg_error(cause));
        current = cause.source();
    }
    src.into_iter().flatten().collect::<Vec<_>>().join("_")
}
fn fmt_dbg_error(e: impl std::error::Error) -> Option<String> {
    let err = format!("{e:?}");
    if let Some((first, _)) = err.split_once('(') {
        Some(first.to_string())
    } else if err.contains(' ') || err.contains("::") {
        None
    } else {
        Some(err)
    }
}

/// Expose all controller components used by main
pub mod controller;
pub use crate::controller::*;

/// Log and trace integrations
pub mod telemetry;

/// Metrics
mod metrics;
pub use metrics::Metrics;

#[cfg(test)] pub mod fixtures;
