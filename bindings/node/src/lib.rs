#![deny(clippy::all)]

use napi_derive::napi;

mod error;
mod session;
mod types;
mod utils;

pub use error::{VectorDBError, Result};
pub use session::VectorDBSession;

#[napi]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[napi(object)]
pub struct PlatformInfo {
    pub platform: String,
    pub arch: String,
}
