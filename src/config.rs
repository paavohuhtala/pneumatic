use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const ONE_MEGABYTE: u64 = 1000000;

const DEFAULT_SMALL_FILE_THRESHOLD: u64 = ONE_MEGABYTE;
const DEFAULT_BUNDLE_TARGET_SIZE: u64 = 64 * ONE_MEGABYTE;
const DEFAULT_LARGE_FILE_THRESHOLD: u64 = 512 * ONE_MEGABYTE;

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub roots: Vec<PathBuf>,
    // TODO: Replace with number_prefix?
    pub small_file_threshold_bytes: Option<u64>,
    pub large_file_threshold_bytes: Option<u64>,
    pub bundle_target_size: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            roots: Vec::new(),
            small_file_threshold_bytes: Some(DEFAULT_SMALL_FILE_THRESHOLD),
            large_file_threshold_bytes: Some(DEFAULT_LARGE_FILE_THRESHOLD),
            bundle_target_size: Some(DEFAULT_BUNDLE_TARGET_SIZE),
        }
    }
}

impl ServerConfig {
    pub fn get_small_file_threshold(&self) -> u64 {
        self.small_file_threshold_bytes
            .unwrap_or(DEFAULT_SMALL_FILE_THRESHOLD)
    }
    pub fn get_large_file_threshold(&self) -> u64 {
        self.large_file_threshold_bytes
            .unwrap_or(DEFAULT_LARGE_FILE_THRESHOLD)
    }
}
