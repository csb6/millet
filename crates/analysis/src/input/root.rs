//! Deal with roots.

mod group_path;

use crate::input::util::{
  canonicalize, get_path_id, read_dir, ErrorSource, GetInputErrorKind, GroupPathKind, InputError,
  Result,
};
use group_path::GroupPath;
use paths::PathId;
use std::path::{Path, PathBuf};

/// The root, in which every path is contained.
#[derive(Debug)]
pub struct Root {
  paths: paths::Root,
  group_path: Option<GroupPath>,
}

impl Root {
  /// Given a path to either a group path or a directory, return the root for it.
  pub fn from_path<F>(fs: &F, path: &Path) -> Result<Self>
  where
    F: paths::FileSystem,
  {
    let path = canonicalize(fs, path, &ErrorSource::default())?;
    let (root_path, group_path) = match GroupPath::new(fs, path.clone().into_path_buf()) {
      None => (path, None),
      Some(group_path) => {
        let parent = group_path.as_path().parent().expect("group path has no parent");
        let rp =
          canonicalize(fs, parent, &ErrorSource { path: Some(path.into_path_buf()), range: None })?;
        (rp, Some(group_path))
      }
    };
    Ok(Self { paths: paths::Root::new(root_path), group_path })
  }

  /// Get this from a canonical root dir.
  pub fn from_canonical_dir(path: paths::CanonicalPathBuf) -> Self {
    Self { paths: paths::Root::new(path), group_path: None }
  }

  /// Returns this as a paths root.
  pub fn as_paths(&self) -> &paths::Root {
    &self.paths
  }

  /// Returns this as a mutable paths root.
  pub fn as_mut_paths(&mut self) -> &mut paths::Root {
    &mut self.paths
  }
}

pub(crate) struct RootGroupPath {
  pub(crate) path: PathId,
  pub(crate) kind: GroupPathKind,
  pub(crate) path_vars: paths::slash_var_path::Env,
}

impl RootGroupPath {
  pub(crate) fn new<F>(fs: &F, root: &mut Root) -> Result<Self>
  where
    F: paths::FileSystem,
  {
    let mut root_group_source = ErrorSource::default();
    let config_path = root.paths.as_path().join(config::FILE_NAME);
    let path_vars = match fs.read_to_string(&config_path) {
      Ok(contents) => {
        let config = Config::new(fs, root, config_path, contents.as_str())?;
        if let Some(path) = config.root_group {
          root.group_path = Some(path);
          root_group_source.path = Some(config.path);
        }
        config.path_vars
      }
      Err(_) => paths::slash_var_path::Env::default(),
    };
    // if not, try to get one from the root dir.
    if root.group_path.is_none() {
      let dir_entries = read_dir(fs, ErrorSource::default(), root.paths.as_path())?;
      for entry in dir_entries {
        if let Some(group_path) = GroupPath::new(fs, entry.clone()) {
          match &root.group_path {
            Some(rgp) => {
              return Err(InputError {
                kind: GetInputErrorKind::MultipleRoots(rgp.as_path().to_owned(), entry.clone()),
                source: ErrorSource { path: Some(rgp.as_path().to_owned()), range: None },
                path: entry,
              })
            }
            None => root.group_path = Some(group_path),
          }
        }
      }
    }
    let root_group_path = root.group_path.as_ref().ok_or_else(|| InputError {
      source: ErrorSource::default(),
      path: root.paths.as_path().to_owned(),
      kind: GetInputErrorKind::NoRoot,
    })?;
    Ok(Self {
      path: get_path_id(fs, &mut root.paths, root_group_source, root_group_path.as_path())?,
      kind: root_group_path.kind(),
      path_vars,
    })
  }
}

struct Config {
  path: PathBuf,
  path_vars: paths::slash_var_path::Env,
  root_group: Option<GroupPath>,
}

impl Config {
  fn new<F>(fs: &F, root: &Root, config_path: PathBuf, contents: &str) -> Result<Self>
  where
    F: paths::FileSystem,
  {
    let mut ret = Self {
      path: config_path,
      path_vars: paths::slash_var_path::Env::default(),
      root_group: None,
    };
    let parsed: config::Root = match toml::from_str(contents) {
      Ok(x) => x,
      Err(e) => {
        return Err(InputError {
          source: ErrorSource::default(),
          path: ret.path,
          kind: GetInputErrorKind::CouldNotParseConfig(e),
        })
      }
    };
    if parsed.version != 1 {
      return Err(InputError {
        source: ErrorSource::default(),
        path: ret.path,
        kind: GetInputErrorKind::InvalidConfigVersion(parsed.version),
      });
    }
    let ws = match parsed.workspace {
      Some(x) => x,
      None => return Ok(ret),
    };
    if let Some(members) = ws.members {
      if ws.root.is_some() || ws.path_vars.is_some() {
        return Err(InputError {
          source: ErrorSource::default(),
          path: ret.path,
          kind: GetInputErrorKind::HasMembersButAlsoRootOrPathVars,
        });
      }
      // TODO
      log::error!("unsupported use of members: {members:?}");
    }
    if let Some(ws_path_vars) = ws.path_vars {
      for (key, val) in ws_path_vars {
        match val {
          config::PathVar::Value(val) => {
            ret.path_vars.insert(key, val);
          }
          config::PathVar::Path(p) => {
            let val: str_util::SmolStr =
              root.paths.as_path().join(p.as_str()).to_string_lossy().into();
            ret.path_vars.insert(key, val);
          }
        }
      }
    }
    // try to get from the config.
    if let (None, Some(path)) = (&root.group_path, ws.root) {
      let path = root.paths.as_path().join(path.as_str());
      match GroupPath::new(fs, path.clone()) {
        Some(path) => ret.root_group = Some(path),
        None => {
          return Err(InputError {
            source: ErrorSource { path: Some(ret.path), range: None },
            path,
            kind: GetInputErrorKind::NotGroup,
          })
        }
      }
    }
    Ok(ret)
  }
}
