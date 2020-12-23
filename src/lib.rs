#![warn(rust_2018_idioms)]
#![allow(unused_imports)]

use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to patch Foo: {}", source))]
    FooPatchFailed {
        source: kube::Error,
        backtrace: Backtrace,
    },

    SerializationFailed {
        source: serde_json::Error,
        backtrace: Backtrace,
    },
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// State machinery for kube, as exposeable to actix
pub mod manager;
pub use manager::Manager;

/// Generated type, for crdgen
pub use manager::Foo;
