//! High-level Intermediate Representation.

#![deny(clippy::pedantic, missing_debug_implementations, rust_2018_idioms)]

use std::fmt;

use la_arena::Arena;
use str_util::{Name, SmolStr};

pub use la_arena;
pub use num_bigint::{BigInt, ParseBigIntError};

#[derive(Debug, Default, Clone)]
pub struct Arenas {
  pub str_dec: StrDecArena,
  pub str_exp: StrExpArena,
  pub sig_exp: SigExpArena,
  pub spec: SpecArena,
  pub exp: ExpArena,
  pub dec: DecArena,
  pub pat: PatArena,
  pub ty: TyArena,
}

macro_rules! mk_idx {
  ( $( $name:ident ),* $(,)? ) => {
    #[doc = "An index into an arena."]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Idx {
      $($name(la_arena::Idx<$name>),)*
    }

    $(
      impl From<la_arena::Idx<$name>> for Idx {
        fn from(val: la_arena::Idx<$name>) -> Self {
          Self::$name(val)
        }
      }
    )*
  };
}

mk_idx![StrDec, StrExp, SigExp, Spec, Exp, Dec, Pat, Ty];

pub type OptIdx<T> = Option<la_arena::Idx<T>>;

// modules //

#[derive(Debug, Clone)]
pub struct SigBind {
  pub name: Name,
  pub sig_exp: SigExpIdx,
}

#[derive(Debug, Clone)]
pub struct FunctorBind {
  pub functor_name: Name,
  pub param_name: Name,
  pub param_sig: SigExpIdx,
  pub body: StrExpIdx,
}

pub type StrDecIdx = OptIdx<StrDec>;
pub type StrDecArena = Arena<StrDec>;

/// Def(87) is handled by not distinguishing between top decs and str decs.
#[derive(Debug, Clone)]
pub enum StrDec {
  Dec(DecIdx),
  Structure(Vec<StrBind>),
  Local(StrDecIdx, StrDecIdx),
  Seq(Vec<StrDecIdx>),
  /// technically a top dec in the Definition.
  Signature(Vec<SigBind>),
  /// technically a top dec in the Definition.
  Functor(Vec<FunctorBind>),
}

#[derive(Debug, Clone)]
pub struct StrBind {
  pub name: Name,
  pub str_exp: StrExpIdx,
}

pub type StrExpIdx = OptIdx<StrExp>;
pub type StrExpArena = Arena<StrExp>;

#[derive(Debug, Clone)]
pub enum StrExp {
  Struct(StrDecIdx),
  Path(Path),
  Ascription(StrExpIdx, Ascription, SigExpIdx),
  App(Name, StrExpIdx),
  Let(StrDecIdx, StrExpIdx),
}

#[derive(Debug, Clone)]
pub enum Ascription {
  Transparent,
  Opaque,
}

pub type SigExpIdx = OptIdx<SigExp>;
pub type SigExpArena = Arena<SigExp>;

#[derive(Debug, Clone)]
pub enum SigExp {
  Spec(SpecIdx),
  Name(Name),
  WhereType(SigExpIdx, Vec<TyVar>, Path, TyIdx),
  /// not in the Definition
  Where(SigExpIdx, Path, Path),
}

pub type SpecIdx = OptIdx<Spec>;
pub type SpecArena = Arena<Spec>;

#[derive(Debug, Clone)]
pub enum Spec {
  /// the `Vec<TyVar>` will always be empty from the source, but may be filled in implicitly.
  Val(Vec<TyVar>, Vec<ValDesc>),
  Ty(TyDesc),
  EqTy(TyDesc),
  Datatype(DatDesc),
  DatatypeCopy(Name, Path),
  Exception(ExDesc),
  Str(StrDesc),
  Include(SigExpIdx),
  Sharing(SpecIdx, SharingKind, Vec<Path>),
  Seq(Vec<SpecIdx>),
}

#[derive(Debug, Clone)]
pub enum SharingKind {
  /// The non-derived form, `sharing type`.
  Regular,
  /// The derived form, `sharing`. Though this is a derived form, we represent it in HIR because
  /// lowering it requires non-trivial statics information.
  Derived,
}

#[derive(Debug, Clone)]
pub struct ValDesc {
  pub name: Name,
  pub ty: TyIdx,
}

#[derive(Debug, Clone)]
pub struct TyDesc {
  pub ty_vars: Vec<TyVar>,
  pub name: Name,
}

pub type DatDesc = DatBind;
pub type ConDesc = ConBind;

#[derive(Debug, Clone)]
pub struct ExDesc {
  pub name: Name,
  pub ty: Option<TyIdx>,
}

#[derive(Debug, Clone)]
pub struct StrDesc {
  pub name: Name,
  pub sig_exp: SigExpIdx,
}

// core //

pub type ExpIdx = OptIdx<Exp>;
pub type ExpArena = Arena<Exp>;

/// Def(7) is handled by having no distinction between atomic expressions and others here.
#[derive(Debug, Clone)]
pub enum Exp {
  Hole,
  SCon(SCon),
  Path(Path),
  Record(Vec<(Lab, ExpIdx)>),
  Let(DecIdx, ExpIdx),
  App(ExpIdx, ExpIdx),
  Handle(ExpIdx, Vec<(PatIdx, ExpIdx)>),
  Raise(ExpIdx),
  Fn(Vec<(PatIdx, ExpIdx)>),
  Typed(ExpIdx, TyIdx),
}

pub type DecIdx = OptIdx<Dec>;
pub type DecArena = Arena<Dec>;

#[derive(Debug, Clone)]
pub enum Dec {
  Val(Vec<TyVar>, Vec<ValBind>),
  Ty(Vec<TyBind>),
  /// The TyBinds are from `withtype`, since it's easier to process in statics than lower.
  Datatype(Vec<DatBind>, Vec<TyBind>),
  DatatypeCopy(Name, Path),
  /// The TyBinds are from `withtype`, since it's easier to process in statics than lower.
  Abstype(Vec<DatBind>, Vec<TyBind>, DecIdx),
  Exception(Vec<ExBind>),
  Local(DecIdx, DecIdx),
  Open(Vec<Path>),
  Seq(Vec<DecIdx>),
}

#[derive(Debug, Clone)]
pub struct ValBind {
  pub rec: bool,
  pub pat: PatIdx,
  pub exp: ExpIdx,
}

#[derive(Debug, Clone)]
pub struct TyBind {
  pub ty_vars: Vec<TyVar>,
  pub name: Name,
  pub ty: TyIdx,
}

#[derive(Debug, Clone)]
pub struct DatBind {
  pub ty_vars: Vec<TyVar>,
  pub name: Name,
  pub cons: Vec<ConBind>,
}

#[derive(Debug, Clone)]
pub struct ConBind {
  pub name: Name,
  pub ty: Option<TyIdx>,
}

#[derive(Debug, Clone)]
pub enum ExBind {
  New(Name, Option<TyIdx>),
  Copy(Name, Path),
}

pub type PatIdx = OptIdx<Pat>;
pub type PatArena = Arena<Pat>;

/// Def(40) is handled by having no distinction between atomic expressions and others here.
#[derive(Debug, Clone)]
pub enum Pat {
  Wild,
  SCon(SCon),
  Con(Path, Option<PatIdx>),
  Record {
    rows: Vec<(Lab, PatIdx)>,
    allows_other: bool,
  },
  Typed(PatIdx, TyIdx),
  /// the Definition defines as-pats as being a name, then an optional type annotation, then `as`,
  /// then a pat. however, we put the type annotation on the rhs pat.
  ///
  /// also note that we parse `<pat> as <pat>` but reject when lowering.
  As(Name, PatIdx),
  Or(OrPat),
}

#[derive(Debug, Clone)]
pub struct OrPat {
  pub first: PatIdx,
  pub rest: Vec<PatIdx>,
}

pub type TyIdx = OptIdx<Ty>;
pub type TyArena = Arena<Ty>;

#[derive(Debug, Clone)]
pub enum Ty {
  Hole,
  Var(TyVar),
  Record(Vec<(Lab, TyIdx)>),
  Con(Vec<TyIdx>, Path),
  Fn(TyIdx, TyIdx),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lab {
  Name(Name),
  Num(usize),
}

impl fmt::Display for Lab {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Name(name) => name.fmt(f),
      Self::Num(n) => n.fmt(f),
    }
  }
}

impl Lab {
  #[must_use]
  pub fn tuple(idx: usize) -> Self {
    Self::Num(idx + 1)
  }
}

#[derive(Debug, Clone)]
pub enum SCon {
  Int(Int),
  Real(f64),
  Word(u64),
  Char(char),
  String(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Int {
  Finite(i32),
  Big(num_bigint::BigInt),
}

impl fmt::Display for Int {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Int::Finite(x) => x.fmt(f),
      Int::Big(x) => x.fmt(f),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
  structures: Vec<Name>,
  last: Name,
}

impl Path {
  pub fn new<I>(structures: I, last: Name) -> Self
  where
    I: IntoIterator<Item = Name>,
  {
    Self { structures: structures.into_iter().collect(), last }
  }

  #[must_use]
  pub fn try_new(mut names: Vec<Name>) -> Option<Self> {
    let last = names.pop()?;
    Some(Self::new(names, last))
  }

  #[must_use]
  pub fn one(name: Name) -> Self {
    Self::new(Vec::new(), name)
  }

  #[must_use]
  pub fn last(&self) -> &Name {
    &self.last
  }

  #[must_use]
  pub fn structures(&self) -> &[Name] {
    &self.structures
  }

  pub fn all_names(&self) -> impl Iterator<Item = &Name> {
    self.structures.iter().chain(std::iter::once(&self.last))
  }
}

impl fmt::Display for Path {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for structure in &self.structures {
      structure.fmt(f)?;
      f.write_str(".")?;
    }
    self.last.fmt(f)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TyVar(Name);

impl TyVar {
  pub fn new<S>(s: S) -> Self
  where
    S: Into<SmolStr>,
  {
    Self(Name::new(s))
  }

  #[must_use]
  pub fn is_equality(&self) -> bool {
    self.0.as_str().as_bytes().get(1) == Some(&b'\'')
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }

  #[must_use]
  pub fn as_name(&self) -> &Name {
    &self.0
  }
}

impl fmt::Display for TyVar {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0)
  }
}
