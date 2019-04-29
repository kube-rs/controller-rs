#![warn(rust_2018_idioms)]
#![allow(unused_imports)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate failure;

pub use failure::Error;
pub type Result<T> = std::result::Result<T, Error>;

/// State machinery for kube, as exposeable to actix
pub mod state;
pub use state::State;
