//! External access to the Surver server.
use std::{sync::LazyLock, time::SystemTime};

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
pub use server::surver_main;

pub const HTTP_SERVER_KEY: &str = "Server";
pub const HTTP_SERVER_VALUE_SURFER: &str = "Surfer";
pub const X_WELLEN_VERSION: &str = "x-wellen-version";
pub const X_SURFER_VERSION: &str = "x-surfer-version";
pub const SURFER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WELLEN_VERSION: &str = wellen::VERSION;

pub const WELLEN_SURFER_DEFAULT_OPTIONS: wellen::LoadOptions = wellen::LoadOptions {
    multi_thread: true,
    remove_scopes_with_empty_name: true,
};

#[derive(Debug, Deserialize)]
pub struct SurverConfig {
    /// IP address to bind the HTTP server to
    pub bind_address: String,
    /// Default port for the HTTP server
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SurverStatus {
    pub wellen_version: String,
    pub surfer_version: String,
    pub file_infos: Vec<SurverFileInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SurverFileInfo {
    pub bytes: u64,
    pub bytes_loaded: u64,
    pub filename: String,
    pub format: wellen::FileFormat,
    pub reloading: bool,
    pub last_load_ok: bool,
    pub last_modification_time: Option<SystemTime>,
}

impl SurverFileInfo {
    pub fn modification_time_string(&self) -> String {
        modification_time_string(self.last_modification_time)
    }
}

pub static BINCODE_OPTIONS: LazyLock<bincode::DefaultOptions> =
    LazyLock::new(bincode::DefaultOptions::new);

pub(crate) fn modification_time_string(mtime: Option<SystemTime>) -> String {
    if let Some(mtime) = mtime {
        let dur = mtime
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        return chrono::DateTime::<chrono::Utc>::from_timestamp(
            dur.as_secs() as i64,
            dur.subsec_nanos(),
        )
        .map_or_else(
            || "Incorrect timestamp".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        );
    }
    "unknown".to_string()
}
