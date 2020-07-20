#![warn(rust_2018_idioms)]
#![allow(unused_imports)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate prometheus;
use snafu::{Backtrace, OptionExt, ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Foo has bad info: {}", info))]
    FooIsBad { info: String, backtrace: Backtrace },
}

pub type Result<T> = std::result::Result<T, Error>;

/// State machinery for kube, as exposeable to actix
pub mod state;
pub use state::Manager;
