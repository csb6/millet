//! See [`Disallow`].

use crate::item::Item;
use std::fmt;

/// A way in which something is not allowed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Disallow {
  /// TODO remove
  #[allow(dead_code)]
  Directly,
}

impl fmt::Display for Disallow {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Disallow::Directly => f.write_str("directly"),
    }
  }
}

/// An error when trying to disallow a path.
#[derive(Debug)]
pub struct Error(ErrorKind);

#[derive(Debug)]
pub(crate) enum ErrorKind {
  Undefined(Item, str_util::Name),
}

impl From<ErrorKind> for Error {
  fn from(value: ErrorKind) -> Self {
    Error(value)
  }
}
