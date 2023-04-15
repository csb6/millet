//! Information about SML Core constructs.

#![allow(clippy::module_name_repetitions)]

use crate::sym::Exn;
use crate::ty::TyScheme;
use crate::{def, disallow::Disallow};
use chain_map::ChainMap;
use fast_hash::FxHashSet;

/// Information about a type.
///
/// Definition: `TyStr`
#[derive(Debug, Clone)]
pub struct TyInfo {
  /// The type scheme.
  pub ty_scheme: TyScheme,
  /// The val environment.
  pub val_env: ValEnv,
  /// The def.
  pub def: Option<def::Def>,
  /// Whether this is disallowed.
  pub disallow: Option<Disallow>,
}

/// Definition: `TyEnv`
pub type TyEnv = ChainMap<str_util::Name, TyInfo>;

/// Definition: `ValEnv`
pub type ValEnv = ChainMap<str_util::Name, ValInfo>;

/// Information about a value.
#[derive(Debug, Clone)]
pub struct ValInfo {
  /// The type scheme for this value.
  pub ty_scheme: TyScheme,
  /// The identifier status.
  pub id_status: IdStatus,
  /// The definitions. It's a set, because we can have or patterns.
  pub defs: FxHashSet<def::Def>,
  /// Whether this is disallowed.
  pub disallow: Option<Disallow>,
}

/// Definition: `IdStatus`
#[derive(Debug, Clone, Copy)]
pub enum IdStatus {
  /// A constructor.
  Con,
  /// An exception.
  Exn(Exn),
  /// A general value.
  Val,
}

impl IdStatus {
  /// Returns whether this is the same kind of identifier status as the other.
  #[must_use]
  pub fn same_kind_as(self, other: Self) -> bool {
    matches!(
      (self, other),
      (Self::Con, Self::Con) | (Self::Exn(_), Self::Exn(_)) | (Self::Val, Self::Val)
    )
  }
}