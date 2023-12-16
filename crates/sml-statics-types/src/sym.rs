//! `Sym`s, aka symbols, aka type names, aka generated types. And exceptions.

#![allow(clippy::module_name_repetitions)]

use crate::info::{TyInfo, ValEnv, ValInfo};
use crate::ty::{Ty, TyKind, TyScheme};
use crate::{def, overload};
use drop_bomb::DropBomb;
use fast_hash::FxHashMap;
use std::fmt;

/// A symbol, aka a type name. Definition: `TyName`
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(idx::Idx);

impl fmt::Debug for Sym {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut dt = f.debug_tuple("Sym");
    match self.primitive() {
      None => dt.field(&self.0),
      Some(x) => dt.field(&x.as_str()),
    };
    dt.finish()
  }
}

macro_rules! mk_special_syms {
  ($( ($idx:expr, $mk_ty:ident, $name:ident, $prim:path), )*) => {
    impl Sym {
      $(
        #[allow(missing_docs)]
        pub const $name: Self = Self(idx::Idx::new_u32($idx));
      )*

      #[doc = "Returns the primitive kind for this sym."]
      #[must_use]
      pub fn primitive(&self) -> Option<def::Primitive> {
        let s = match *self {
          $(
            Self::$name => $prim,
          )*
          _ => return None,
        };
        Some(s)
      }
    }

    impl Ty {
      $(
        mk_special_syms!(@mk_ty, $mk_ty, $name, $idx);
      )*
    }
  };
  (@mk_ty, y, $name:ident, $idx:expr) => {
    #[allow(missing_docs)]
    pub const $name: Self = Self { kind: TyKind::Con, idx: idx::Idx::new_u32($idx) };
  };
  (@mk_ty, n, $name:ident, $idx:expr) => {};
}

// @sync(special_sym_order)
mk_special_syms![
  (0, y, EXN, def::Primitive::Exn),
  (1, y, INT, def::Primitive::Int),
  (2, y, WORD, def::Primitive::Word),
  (3, y, REAL, def::Primitive::Real),
  (4, y, CHAR, def::Primitive::Char),
  (5, y, STRING, def::Primitive::String),
  (6, y, BOOL, def::Primitive::Bool),
  (7, n, LIST, def::Primitive::List),
  (8, n, REF, def::Primitive::RefTy),
  (9, n, VECTOR, def::Primitive::Vector),
];

impl Sym {
  /// there's only 1, and it's EXN.
  const NUM_WEIRD: usize = 1;

  /// never call this on a weird sym.
  fn idx(self) -> usize {
    self.0.to_usize() - Self::NUM_WEIRD
  }

  /// Returns whether this `Sym` was generated by a [`Syms`] after that `Syms` generated the
  /// `marker`.
  #[must_use]
  pub fn generated_after(self, marker: SymsMarker) -> bool {
    self != Self::EXN && self.idx() >= marker.0
  }
}

/// An exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Exn(idx::Idx);

/// Whether this `Sym` admits equality.
#[derive(Debug, Clone, Copy)]
pub enum Equality {
  /// It always does, regardless of whether the type arguments do.
  Always,
  /// It does if and only if the type arguments do.
  Sometimes,
  /// It never does, regardless of whether the type arguments do.
  Never,
}

/// A value environment for `SymInfo`.
#[derive(Debug, Default, Clone)]
pub struct SymValEnv {
  /// insertion order. invariant: forall (k, v) in map, k in order
  order: Vec<str_util::Name>,
  /// actual mapping
  map: FxHashMap<str_util::Name, ValInfo>,
}

impl SymValEnv {
  /// Inserts the key-val mapping into this. Returns whether the key was newly inserted.
  pub fn insert(&mut self, key: str_util::Name, val: ValInfo) -> bool {
    if self.map.insert(key.clone(), val).is_some() {
      false
    } else {
      self.order.push(key);
      true
    }
  }

  /// Returns the val for this key.
  #[must_use]
  pub fn get(&self, key: &str_util::Name) -> Option<&ValInfo> {
    self.map.get(key)
  }

  /// Returns an iterator over the key-value pairs in insertion order.
  pub fn iter(&self) -> impl Iterator<Item = (&str_util::Name, &ValInfo)> + '_ {
    self.order.iter().map(|k| (k, &self.map[k]))
  }

  /// Returns an iterator over the key and mutable value pairs in an arbitrary order.
  pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str_util::Name, &mut ValInfo)> + '_ {
    self.map.iter_mut()
  }
}

impl FromIterator<(str_util::Name, ValInfo)> for SymValEnv {
  fn from_iter<T: IntoIterator<Item = (str_util::Name, ValInfo)>>(iter: T) -> Self {
    let mut ret = SymValEnv::default();
    for (name, val) in iter {
      ret.insert(name, val);
    }
    ret
  }
}

impl From<SymValEnv> for ValEnv {
  fn from(value: SymValEnv) -> Self {
    value.map.into_iter().collect()
  }
}

/// A type info for `SymInfo`.
pub type SymTyInfo = TyInfo<SymValEnv>;

/// Information about a `Sym`.
#[derive(Debug, Clone)]
pub struct SymInfo {
  /// The path this sym was defined at.
  pub path: sml_path::Path,
  /// The ty info for the sym.
  pub ty_info: SymTyInfo,
  /// How this sym admits equality.
  pub equality: Equality,
}

/// Information about an `Exn`.
#[derive(Debug, Clone)]
pub struct ExnInfo {
  /// The path the exn was declared at.
  pub path: sml_path::Path,
  /// The parameter type for this exception.
  pub param: Option<Ty>,
}

/// Information about overloads.
///
/// Each field is a non-empty vec of symbols.
#[derive(Debug, Default, Clone)]
pub struct Overloads {
  /// Overloads for `int`, like `Int16.int`.
  pub int: Vec<Sym>,
  /// Overloads for `real`, like `Real64.real`.
  pub real: Vec<Sym>,
  /// Overloads for `word`, like `Word8.word`.
  pub word: Vec<Sym>,
  /// Overloads for `string`.
  ///
  /// Usually this has length 1, since there is only one.
  pub string: Vec<Sym>,
  /// Overloads for `char`.
  ///
  /// Usually this has length 1, since there is only one.
  pub char: Vec<Sym>,
}

impl std::ops::Index<overload::Basic> for Overloads {
  type Output = Vec<Sym>;

  fn index(&self, index: overload::Basic) -> &Self::Output {
    match index {
      overload::Basic::Int => &self.int,
      overload::Basic::Real => &self.real,
      overload::Basic::Word => &self.word,
      overload::Basic::String => &self.string,
      overload::Basic::Char => &self.char,
    }
  }
}

impl std::ops::IndexMut<overload::Basic> for Overloads {
  fn index_mut(&mut self, index: overload::Basic) -> &mut Self::Output {
    match index {
      overload::Basic::Int => &mut self.int,
      overload::Basic::Real => &mut self.real,
      overload::Basic::Word => &mut self.word,
      overload::Basic::String => &mut self.string,
      overload::Basic::Char => &mut self.char,
    }
  }
}

/// Information about generated types, generated exceptions, and overload types.
///
/// Note the `Default` impl is "fake", in that it returns a totally empty `Syms`, which will lack
/// even built-in items like `type int` and `exception Bind`.
#[derive(Debug, Default, Clone)]
pub struct Syms {
  /// always use Sym::idx to index
  syms: Vec<SymInfo>,
  exns: Vec<ExnInfo>,
  overloads: Overloads,
}

impl Syms {
  /// Start constructing a `Sym`.
  pub fn start(&mut self, path: sml_path::Path) -> StartedSym {
    let ty_info = TyInfo {
      ty_scheme: TyScheme::zero(Ty::NONE),
      val_env: SymValEnv::default(),
      defs: def::Set::default(),
      disallow: None,
    };
    // must start with sometimes equality, as an assumption for constructing datatypes. we may
    // realize that it should actually be never equality based on arguments to constructors.
    self.syms.push(SymInfo { path, ty_info, equality: Equality::Sometimes });
    StartedSym {
      bomb: DropBomb::new("must be passed to Syms::finish"),
      // calculate len after push, because we sub 1 in get, because of Sym::EXN.
      sym: Sym(idx::Idx::new(self.syms.len())),
    }
  }

  /// Finish constructing a `Sym`.
  pub fn finish(&mut self, mut started: StartedSym, ty_info: SymTyInfo, equality: Equality) {
    started.bomb.defuse();
    let sym_info = &mut self.syms[started.sym.idx()];
    sym_info.ty_info = ty_info;
    sym_info.equality = equality;
  }

  /// Returns `None` if and only if passed `Sym::EXN`.
  ///
  /// # Panics
  ///
  /// If the sym didn't exist. (Probably a different `Syms` generated it).
  #[must_use]
  pub fn get(&self, sym: Sym) -> Option<&SymInfo> {
    if sym == Sym::EXN {
      return None;
    }
    Some(self.syms.get(sym.idx()).unwrap())
  }

  /// Inserts a new exception.
  pub fn insert_exn(&mut self, path: sml_path::Path, param: Option<Ty>) -> Exn {
    let ret = Exn(idx::Idx::new(self.exns.len()));
    self.exns.push(ExnInfo { path, param });
    ret
  }

  /// Gets information about an exception.
  ///
  /// # Panics
  ///
  /// If the exn didn't exist. (Probably a different `Syms` generated it).
  #[must_use]
  pub fn get_exn(&self, exn: Exn) -> &ExnInfo {
    self.exns.get(exn.0.to_usize()).unwrap()
  }

  /// Return a marker, so we may later whether a `Sym` has been generated after this marker.
  pub fn mark(&self) -> SymsMarker {
    SymsMarker(self.syms.len())
  }

  /// Iterate over the `Syms`'s info.
  pub fn iter_syms(&self) -> impl Iterator<Item = &SymInfo> {
    self.syms.iter()
  }

  /// Returns the overloads.
  pub(crate) fn overloads(&self) -> &Overloads {
    &self.overloads
  }

  /// Returns the mutable overloads.
  pub fn overloads_mut(&mut self) -> &mut Overloads {
    &mut self.overloads
  }
}

/// A marker to determine when a `Sym` was generated.
#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct SymsMarker(usize);

/// A helper to construct information about [`Syms`]s.
#[must_use]
#[derive(Debug)]
pub struct StartedSym {
  bomb: DropBomb,
  sym: Sym,
}

impl StartedSym {
  /// Returns the sym that this marker represents.
  #[must_use]
  pub fn sym(&self) -> Sym {
    self.sym
  }
}
