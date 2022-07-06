//! Configuration.

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]

use serde::Deserialize;

/// The name of the config file.
pub const FILE_NAME: &str = "millet.toml";

/// The root config.
#[derive(Debug, Deserialize)]
pub struct Root {
  /// The version. Should be 1.
  pub version: u16,
  /// The workspace config.
  pub workspace: Option<Workspace>,
}

/// The workspace config.
#[derive(Debug, Deserialize)]
pub struct Workspace {
  /// The root group filename.
  pub root: Option<String>,
}