//! See [`State`].

use anyhow::{anyhow, bail, Result};
use crossbeam_channel::Sender;
use fast_hash::FxHashSet;
use lsp_server::{ExtractError, Message, Notification, ReqQueue, Request, RequestId, Response};
use lsp_types::{notification::Notification as _, Url};
use std::ops::ControlFlow;

pub(crate) fn capabilities() -> lsp_types::ServerCapabilities {
  lsp_types::ServerCapabilities {
    text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
      lsp_types::TextDocumentSyncOptions {
        open_close: Some(true),
        save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
          lsp_types::SaveOptions {
            include_text: Some(true),
          },
        )),
        ..Default::default()
      },
    )),
    ..Default::default()
  }
}

const SOURCE: &str = "millet";
const ERRORS_URL: &str = "https://github.com/azdavis/millet/blob/main/doc/errors.md";
const MAX_FILES_WITH_ERRORS: usize = 20;
const MAX_ERRORS_PER_FILE: usize = 20;

struct Root {
  path: paths::Root,
  has_diagnostics: FxHashSet<Url>,
}

/// The state of the language server. Only this may do IO. (Well, also the [`lsp_server`] channels
/// that communicate over stdin and stdout.)
///
/// TODO: this is horribly inefficient.
pub(crate) struct State {
  root: Option<Root>,
  sender: Sender<Message>,
  req_queue: ReqQueue<(), ()>,
  analysis: analysis::Analysis,
  file_system: paths::RealFileSystem,
}

impl State {
  pub(crate) fn new(root: Option<Url>, sender: Sender<Message>) -> Self {
    let file_system = paths::RealFileSystem::default();
    let mut root = root
      .map(|url| canonical_path_buf(&file_system, &url))
      .transpose();
    let mut ret = Self {
      root: root
        .as_mut()
        .ok()
        .and_then(Option::take)
        .map(|root_path| Root {
          path: paths::Root::new(root_path),
          has_diagnostics: FxHashSet::default(),
        }),
      sender,
      req_queue: ReqQueue::default(),
      analysis: analysis::Analysis::default(),
      file_system,
    };
    if let Err(e) = root {
      ret.show_error(format!("{e:#}"));
    }
    if let Some(root) = &ret.root {
      let glob_pattern = format!(
        "{}/**/*.{{sml,sig,fun,cm,mlb}}",
        root.path.as_path().display()
      );
      ret.send_request::<lsp_types::request::RegisterCapability>(lsp_types::RegistrationParams {
        registrations: vec![lsp_types::Registration {
          id: lsp_types::notification::DidChangeWatchedFiles::METHOD.to_owned(),
          method: lsp_types::notification::DidChangeWatchedFiles::METHOD.to_owned(),
          register_options: Some(
            serde_json::to_value(lsp_types::DidChangeWatchedFilesRegistrationOptions {
              watchers: vec![lsp_types::FileSystemWatcher {
                glob_pattern,
                kind: None,
              }],
            })
            .unwrap(),
          ),
        }],
      });
      ret.publish_diagnostics();
    }
    ret
  }

  fn send_request<R>(&mut self, params: R::Params)
  where
    R: lsp_types::request::Request,
  {
    let req = self
      .req_queue
      .outgoing
      .register(R::METHOD.to_owned(), params, ());
    self.send(req.into())
  }

  #[allow(dead_code)]
  fn send_response(&mut self, res: Response) {
    match self.req_queue.incoming.complete(res.id.clone()) {
      Some(()) => self.send(res.into()),
      None => log::warn!("tried to respond to a non-queued request: {res:?}"),
    }
  }

  fn send_notification<N>(&self, params: N::Params)
  where
    N: lsp_types::notification::Notification,
  {
    let notif = Notification::new(N::METHOD.to_owned(), params);
    self.send(notif.into())
  }

  pub(crate) fn handle_request(&mut self, req: Request) {
    log::debug!("got request: {req:?}");
    self.req_queue.incoming.register(req.id.clone(), ());
    match self.handle_request_(req) {
      ControlFlow::Break(Ok(())) => {}
      ControlFlow::Break(Err(e)) => log::error!("couldn't handle request: {e}"),
      ControlFlow::Continue(req) => log::warn!("unhandled request: {req:?}"),
    }
  }

  fn handle_request_(&mut self, r: Request) -> ControlFlow<Result<()>, Request> {
    ControlFlow::Continue(r)
  }

  pub(crate) fn handle_response(&mut self, res: Response) {
    log::debug!("got response: {res:?}");
    match self.req_queue.outgoing.complete(res.id.clone()) {
      Some(()) => {}
      None => log::warn!("received response for non-queued request: {res:?}"),
    }
  }

  pub(crate) fn handle_notification(&mut self, notif: Notification) {
    log::debug!("got notification: {notif:?}");
    match self.handle_notification_(notif) {
      ControlFlow::Break(Ok(())) => {}
      ControlFlow::Break(Err(e)) => log::error!("couldn't handle notification: {e}"),
      ControlFlow::Continue(notif) => log::warn!("unhandled notification: {notif:?}"),
    }
  }

  fn handle_notification_(&mut self, mut n: Notification) -> ControlFlow<Result<()>, Notification> {
    n = try_notification::<lsp_types::notification::DidChangeWatchedFiles, _>(n, |_| {
      self.publish_diagnostics();
    })?;
    n = try_notification::<lsp_types::notification::DidOpenTextDocument, _>(n, |params| {
      if self.root.is_some() {
        return;
      }
      let url = params.text_document.uri;
      let text = params.text_document.text;
      self.publish_diagnostics_one(url, &text)
    })?;
    n = try_notification::<lsp_types::notification::DidSaveTextDocument, _>(n, |params| {
      if self.root.is_some() {
        return;
      }
      let url = params.text_document.uri;
      if let Some(text) = params.text {
        self.publish_diagnostics_one(url, &text)
      }
    })?;
    n = try_notification::<lsp_types::notification::DidCloseTextDocument, _>(n, |params| {
      if self.root.is_some() {
        return;
      }
      let url = params.text_document.uri;
      self.send_diagnostics(url, Vec::new());
    })?;
    ControlFlow::Continue(n)
  }

  fn show_error(&mut self, message: String) {
    self.send_notification::<lsp_types::notification::ShowMessage>(lsp_types::ShowMessageParams {
      typ: lsp_types::MessageType::ERROR,
      message,
    });
  }

  fn send(&self, msg: Message) {
    log::debug!("sending {msg:?}");
    self.sender.send(msg).unwrap()
  }

  // diagnostics //

  fn publish_diagnostics(&mut self) -> bool {
    let mut root = match self.root.take() {
      Some(x) => x,
      None => return false,
    };
    let mut has_diagnostics = FxHashSet::<Url>::default();
    let input = elapsed::log("get_input", || {
      analysis::get_input(&self.file_system, &mut root.path)
    });
    let input = match input {
      Ok(x) => x,
      Err(e) => {
        log::error!("could not get input: {e}");
        self.show_error(format!("{e}"));
        self.root = Some(root);
        return false;
      }
    };
    let got_many = elapsed::log("get_many", || self.analysis.get_many(&input));
    for (path_id, errors) in got_many {
      let pos_db = match input.get_source(path_id) {
        Some(s) => text_pos::PositionDb::new(s),
        None => {
          log::error!("no contents for {path_id:?}");
          continue;
        }
      };
      let path = root.path.get_path(path_id);
      let url = match Url::parse(&format!("file://{}", path.as_path().display())) {
        Ok(x) => x,
        Err(_) => continue,
      };
      let ds = diagnostics(&pos_db, errors);
      if ds.is_empty() || has_diagnostics.len() >= MAX_FILES_WITH_ERRORS {
        continue;
      }
      has_diagnostics.insert(url.clone());
      self.send_diagnostics(url, ds);
    }
    // this is the old one.
    for url in root.has_diagnostics {
      // this is the new one.
      if has_diagnostics.contains(&url) {
        // had old diagnostics, and has new diagnostics. we just sent the new ones.
        continue;
      }
      // had old diagnostics, but no new diagnostics. clear the old diagnostics.
      self.send_diagnostics(url, Vec::new());
    }
    root.has_diagnostics = has_diagnostics;
    self.root = Some(root);
    true
  }

  fn publish_diagnostics_one(&mut self, url: Url, text: &str) {
    let pos_db = text_pos::PositionDb::new(text);
    self.send_diagnostics(url, diagnostics(&pos_db, self.analysis.get_one(text)));
  }

  fn send_diagnostics(&mut self, url: Url, diagnostics: Vec<lsp_types::Diagnostic>) {
    self.send_notification::<lsp_types::notification::PublishDiagnostics>(
      lsp_types::PublishDiagnosticsParams {
        uri: url,
        diagnostics,
        version: None,
      },
    );
  }
}

#[allow(dead_code)]
fn try_request<R, F>(req: Request, f: F) -> ControlFlow<Result<()>, Request>
where
  R: lsp_types::request::Request,
  F: FnOnce(RequestId, R::Params),
{
  match req.extract::<R::Params>(R::METHOD) {
    Ok((id, params)) => {
      f(id, params);
      ControlFlow::Break(Ok(()))
    }
    Err(e) => extract_error(e),
  }
}

fn try_notification<N, F>(notif: Notification, f: F) -> ControlFlow<Result<()>, Notification>
where
  N: lsp_types::notification::Notification,
  F: FnOnce(N::Params),
{
  match notif.extract::<N::Params>(N::METHOD) {
    Ok(params) => {
      f(params);
      ControlFlow::Break(Ok(()))
    }
    Err(e) => extract_error(e),
  }
}

fn extract_error<T>(e: ExtractError<T>) -> ControlFlow<Result<()>, T> {
  match e {
    ExtractError::MethodMismatch(x) => ControlFlow::Continue(x),
    ExtractError::JsonError { method, error } => {
      ControlFlow::Break(Err(anyhow!("couldn't deserialize for {method}: {error}")))
    }
  }
}

fn canonical_path_buf<F>(fs: &F, url: &Url) -> Result<paths::CanonicalPathBuf>
where
  F: paths::FileSystem,
{
  if url.scheme() != "file" {
    bail!("not a file url")
  }
  match url.to_file_path() {
    Ok(pb) => Ok(fs.canonicalize(pb.as_path())?),
    Err(()) => bail!("invalid url"),
  }
}

fn diagnostics(
  pos_db: &text_pos::PositionDb,
  errors: Vec<analysis::Error>,
) -> Vec<lsp_types::Diagnostic> {
  errors
    .into_iter()
    .take(MAX_ERRORS_PER_FILE)
    .map(|err| {
      let code_description = match Url::parse(&format!("{ERRORS_URL}#{}", err.code)) {
        Ok(href) => Some(lsp_types::CodeDescription { href }),
        Err(e) => {
          log::error!("url parse failed for {err:?}: {e}");
          None
        }
      };
      lsp_types::Diagnostic {
        range: lsp_range(pos_db.range(err.range)),
        message: err.message,
        source: Some(SOURCE.to_owned()),
        severity: Some(lsp_types::DiagnosticSeverity::ERROR),
        code: Some(lsp_types::NumberOrString::Number(err.code.into())),
        code_description,
        ..Default::default()
      }
    })
    .collect()
}

fn lsp_range(range: text_pos::Range) -> lsp_types::Range {
  lsp_types::Range {
    start: lsp_position(range.start),
    end: lsp_position(range.end),
  }
}

fn lsp_position(pos: text_pos::Position) -> lsp_types::Position {
  lsp_types::Position {
    line: pos.line,
    character: pos.character,
  }
}
