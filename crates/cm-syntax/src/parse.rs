use crate::types::{
  Class, DescKind, Error, ErrorKind, Export, Member, Namespace, ParseRoot, PathOrMinus,
  PathOrStdBasis, Result, Token,
};
use text_size_util::{TextRange, WithRange};

pub(crate) fn get(
  tokens: &[WithRange<Token<'_>>],
  env: &paths::slash_var_path::Env,
) -> Result<ParseRoot> {
  let mut p = Parser { tokens, idx: 0, last_range: TextRange::default(), env };
  root(&mut p)
}

struct Parser<'a> {
  tokens: &'a [WithRange<Token<'a>>],
  idx: usize,
  last_range: TextRange,
  env: &'a paths::slash_var_path::Env,
}

impl<'a> Parser<'a> {
  fn cur_tok(&self) -> Option<WithRange<Token<'a>>> {
    self.tokens.get(self.idx).copied()
  }

  fn cur(&self) -> Option<Token<'a>> {
    self.cur_tok().map(|x| x.val)
  }

  fn err<T>(&self, kind: ErrorKind) -> Result<T> {
    Err(Error::new(kind, self.last_range))
  }

  fn string(&self) -> Result<WithRange<&'a str>> {
    match self.cur_tok() {
      Some(tok) => match tok.val {
        Token::String(s) => Ok(tok.wrap(s)),
        _ => self.err(ErrorKind::ExpectedString),
      },
      _ => self.err(ErrorKind::ExpectedString),
    }
  }

  fn bump(&mut self) {
    if let Some(tok) = self.cur_tok() {
      self.last_range = tok.range;
      self.idx += 1;
    }
  }

  fn eat(&mut self, kind: Token<'static>) -> Result<()> {
    if self.cur() == Some(kind) {
      self.bump();
      Ok(())
    } else {
      self.err(ErrorKind::Expected(kind))
    }
  }
}

fn root(p: &mut Parser<'_>) -> Result<ParseRoot> {
  let ret = match p.cur() {
    Some(Token::Group) => {
      p.bump();
      let (exports, members) = exports_and_members(p)?;
      ParseRoot { kind: DescKind::Group, exports, members }
    }
    Some(Token::Library) => {
      p.bump();
      let (exports, members) = exports_and_members(p)?;
      if exports.is_empty() {
        return p.err(ErrorKind::EmptyExportList);
      }
      ParseRoot { kind: DescKind::Library, exports, members }
    }
    _ => return p.err(ErrorKind::ExpectedDesc),
  };
  Ok(ret)
}

fn exports_and_members(p: &mut Parser<'_>) -> Result<(Vec<Export>, Vec<Member>)> {
  let mut exports = Vec::<Export>::new();
  loop {
    let tok = p.cur_tok();
    let tok = match tok {
      Some(x) => x,
      None => break,
    };
    let export = match tok.val {
      Token::Structure => name_export(p, tok, Namespace::Structure)?,
      Token::Signature => name_export(p, tok, Namespace::Signature)?,
      Token::Functor => name_export(p, tok, Namespace::Functor)?,
      Token::FunSig => name_export(p, tok, Namespace::FunSig)?,
      Token::Library => {
        p.bump();
        p.eat(Token::LRound)?;
        let s = p.string()?;
        p.bump();
        let pathname = path(p, s.val)?;
        p.eat(Token::RRound)?;
        Export::Library(s.wrap(pathname))
      }
      Token::Source => {
        p.bump();
        p.eat(Token::LRound)?;
        let path = source_or_group_export_arg(p)?;
        p.eat(Token::RRound)?;
        Export::Source(tok.wrap(path))
      }
      Token::Group => {
        p.bump();
        p.eat(Token::LRound)?;
        let path = source_or_group_export_arg(p)?;
        p.eat(Token::RRound)?;
        Export::Group(tok.wrap(path))
      }
      Token::Is => break,
      _ => {
        p.bump();
        return p.err(ErrorKind::ExpectedExport);
      }
    };
    exports.push(export);
  }
  p.eat(Token::Is)?;
  let mut members = Vec::<Member>::new();
  loop {
    let tok = p.cur_tok();
    let tok = match tok {
      Some(x) => x,
      None => break,
    };
    let s = match tok.val {
      Token::String(s) => s,
      _ => break,
    };
    p.bump();
    let pathname = path(p, s)?;
    let class = match p.cur() {
      Some(Token::Colon) => {
        p.bump();
        let s = p.string()?;
        let c = match s.val.parse::<Class>() {
          Ok(c) => c,
          Err(e) => match e {},
        };
        p.bump();
        Some(s.wrap(c))
      }
      _ => None,
    };
    members.push(Member { pathname: tok.wrap(pathname), class });
  }
  Ok((exports, members))
}

fn source_or_group_export_arg(p: &mut Parser<'_>) -> Result<PathOrMinus> {
  match p.cur() {
    Some(Token::Minus) => {
      p.bump();
      Ok(PathOrMinus::Minus)
    }
    Some(Token::String(s)) => {
      p.bump();
      match path(p, s)? {
        PathOrStdBasis::Path(x) => Ok(PathOrMinus::Path(x)),
        PathOrStdBasis::StdBasis => p.err(ErrorKind::ExpectedPathOrMinus),
      }
    }
    _ => p.err(ErrorKind::ExpectedPathOrMinus),
  }
}

fn name_export(p: &mut Parser<'_>, tok: WithRange<Token<'_>>, ns: Namespace) -> Result<Export> {
  p.bump();
  let s = p.string()?;
  let name = str_util::Name::new(s.val);
  p.bump();
  Ok(Export::Name(tok.wrap(ns), s.wrap(name)))
}

fn path(p: &Parser<'_>, s: &str) -> Result<PathOrStdBasis> {
  match paths::slash_var_path::get(s, p.env) {
    Ok(x) => Ok(PathOrStdBasis::Path(x)),
    Err(e) => {
      if let paths::slash_var_path::Error::Undefined(var) = &e {
        if matches!(var.as_str(), "" | "SMLNJ-LIB") {
          return Ok(PathOrStdBasis::StdBasis);
        }
      }
      p.err(ErrorKind::SlashVarPathError(e))
    }
  }
}
