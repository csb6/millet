//! The unification of all the passes into a single high-level API.

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]

mod std_basis;

use fast_hash::FxHashSet;
use paths::{PathId, PathMap};
use statics::Mode::Regular;
use std::fmt;
use syntax::{ast::AstNode as _, rowan::TextRange};

pub use std_basis::StdBasis;

/// An error.
#[derive(Debug)]
pub struct Error {
  /// The range of the error.
  pub range: TextRange,
  /// The message of the error.
  pub message: String,
}

/// A group of source files.
///
/// TODO use exports
#[derive(Debug)]
pub struct Group {
  /// The source file paths, in order.
  pub source_files: Vec<PathId>,
  /// The dependencies of this group on other groups.
  pub dependencies: FxHashSet<PathId>,
}

/// The input to analysis.
#[derive(Debug, Default)]
pub struct Input {
  /// A map from source files to their contents.
  pub sources: PathMap<String>,
  /// A map from group files to their (parsed) contents.
  pub groups: PathMap<Group>,
}

/// Performs analysis.
#[derive(Debug, Default)]
pub struct Analysis {
  std_basis: StdBasis,
}

impl Analysis {
  /// Returns a new `Analysis`.
  pub fn new(std_basis: StdBasis) -> Self {
    Self { std_basis }
  }

  /// Given the contents of one isolated file, return the errors for it.
  pub fn get_one(&self, s: &str) -> Vec<Error> {
    let mut f = AnalyzedFile::new(s);
    let mut st = self.std_basis.into_statics();
    statics::get(&mut st, Regular, &f.lowered.arenas, &f.lowered.top_decs);
    f.statics_errors = std::mem::take(&mut st.errors);
    f.into_errors(&st.syms).collect()
  }

  /// Given information about many interdependent source files and their groupings, returns a
  /// mapping from source paths to errors.
  pub fn get_many(&self, input: &Input) -> PathMap<Vec<Error>> {
    let graph: topo_sort::Graph<_> = input
      .groups
      .iter()
      .map(|(&path, group)| (path, group.dependencies.iter().copied().collect()))
      .collect();
    // TODO error if cycle
    let order = topo_sort::get(&graph).unwrap_or_default();
    // TODO require explicit basis import
    let mut st = self.std_basis.into_statics();
    order
      .into_iter()
      .flat_map(|path| {
        input
          .groups
          .get(&path)
          .into_iter()
          .flat_map(|x| x.source_files.iter())
      })
      .filter_map(|&path_id| {
        let s = match input.sources.get(&path_id) {
          Some(x) => x,
          None => {
            log::error!("no contents for {path_id:?}");
            return None;
          }
        };
        let mut f = AnalyzedFile::new(s);
        statics::get(&mut st, Regular, &f.lowered.arenas, &f.lowered.top_decs);
        f.statics_errors = std::mem::take(&mut st.errors);
        Some((path_id, f.into_errors(&st.syms).collect()))
      })
      .collect()
  }
}

struct AnalyzedFile {
  lex_errors: Vec<lex::Error>,
  parsed: parse::Parse,
  lowered: lower::Lower,
  statics_errors: Vec<statics::Error>,
}

impl AnalyzedFile {
  fn new(s: &str) -> Self {
    let lexed = lex::get(s);
    log::debug!("lex: {:?}", lexed.tokens);
    let parsed = parse::get(&lexed.tokens);
    log::debug!("parse: {:#?}", parsed.root);
    let mut lowered = lower::get(&parsed.root);
    ty_var_scope::get(&mut lowered.arenas, &lowered.top_decs);
    Self {
      lex_errors: lexed.errors,
      parsed,
      lowered,
      statics_errors: Vec::new(),
    }
  }

  fn into_errors(self, syms: &statics::Syms) -> impl Iterator<Item = Error> + '_ {
    std::iter::empty()
      .chain(self.lex_errors.into_iter().map(|err| Error {
        range: err.range,
        message: err.kind.to_string(),
      }))
      .chain(self.parsed.errors.into_iter().map(|err| Error {
        range: err.range,
        message: err.kind.to_string(),
      }))
      .chain(self.lowered.errors.into_iter().map(|err| Error {
        range: err.range,
        message: err.kind.to_string(),
      }))
      .chain(self.statics_errors.into_iter().filter_map(move |err| {
        Some(Error {
          range: self
            .lowered
            .ptrs
            .get(err.idx())?
            .to_node(self.parsed.root.syntax())
            .text_range(),
          message: err.display(syms).to_string(),
        })
      }))
  }
}

/// An error when getting input.
#[derive(Debug)]
pub struct GetInputError {
  path: std::path::PathBuf,
  kind: GetInputErrorKind,
}

impl GetInputError {
  fn new(path: &std::path::Path, kind: GetInputErrorKind) -> Self {
    Self {
      path: path.to_owned(),
      kind,
    }
  }
}

impl fmt::Display for GetInputError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}: {}", self.path.display(), self.kind)
  }
}

#[derive(Debug)]
enum GetInputErrorKind {
  ReadFile(std::io::Error),
  Cm(cm::Error),
  Canonicalize(std::io::Error),
  NoParent,
  NotInRoot(std::path::StripPrefixError),
}

impl fmt::Display for GetInputErrorKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      GetInputErrorKind::ReadFile(e) => write!(f, "couldn't read file: {e}"),
      GetInputErrorKind::Cm(e) => write!(f, "couldn't process CM file: {e}"),
      GetInputErrorKind::Canonicalize(e) => write!(f, "couldn't canonicalize: {e}"),
      GetInputErrorKind::NoParent => f.write_str("no parent"),
      GetInputErrorKind::NotInRoot(e) => write!(f, "not in root: {e}"),
    }
  }
}

const ROOT_GROUP: &str = "sources.cm";

/// Get some input from the filesystem.
pub fn get_input<F>(fs: &F, root: &mut paths::Root) -> Result<Input, GetInputError>
where
  F: paths::FileSystem,
{
  let mut ret = Input::default();
  let root_group_id = get_path_id(fs, root, root.as_path().join(ROOT_GROUP).as_path())?;
  let mut stack = vec![root_group_id];
  while let Some(path_id) = stack.pop() {
    let path = root.get_path(path_id).as_path();
    let s = read_file(fs, path)?;
    let cm = cm::get(&s).map_err(|e| GetInputError::new(path, GetInputErrorKind::Cm(e)))?;
    let parent = match path.parent() {
      Some(x) => x.to_owned(),
      None => return Err(GetInputError::new(path, GetInputErrorKind::NoParent)),
    };
    let mut source_files = Vec::<paths::PathId>::new();
    for path in cm.sml {
      let path = parent.join(path.as_path());
      let path_id = get_path_id(fs, root, path.as_path())?;
      let s = read_file(fs, path.as_path())?;
      source_files.push(path_id);
      ret.sources.insert(path_id, s);
    }
    let mut dependencies = FxHashSet::<paths::PathId>::default();
    for path in cm.cm {
      let path = parent.join(path.as_path());
      let path_id = get_path_id(fs, root, path.as_path())?;
      stack.push(path_id);
      dependencies.insert(path_id);
    }
    let group = Group {
      source_files,
      dependencies,
    };
    ret.groups.insert(path_id, group);
  }
  Ok(ret)
}

fn get_path_id<F>(
  fs: &F,
  root: &mut paths::Root,
  path: &std::path::Path,
) -> Result<paths::PathId, GetInputError>
where
  F: paths::FileSystem,
{
  let canonical = fs
    .canonicalize(path)
    .map_err(|e| GetInputError::new(path, GetInputErrorKind::Canonicalize(e)))?;
  root
    .get_id(&canonical)
    .map_err(|e| GetInputError::new(path, GetInputErrorKind::NotInRoot(e)))
}

fn read_file<F>(fs: &F, path: &std::path::Path) -> Result<String, GetInputError>
where
  F: paths::FileSystem,
{
  fs.read_to_string(path)
    .map_err(|e| GetInputError::new(path, GetInputErrorKind::ReadFile(e)))
}
