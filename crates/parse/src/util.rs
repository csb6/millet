use syntax::event_parse::{Exited, Parser};
use syntax::SyntaxKind as SK;

pub(crate) fn must<F>(p: &mut Parser<'_, SK>, f: F)
where
  F: FnOnce(&mut Parser<'_, SK>) -> Option<Exited>,
{
  if f(p).is_none() {
    p.error();
  }
}

/// stops if the sep is not found
pub(crate) fn many_sep<F>(p: &mut Parser<'_, SK>, sep: SK, wrap: SK, mut f: F)
where
  F: FnMut(&mut Parser<'_, SK>),
{
  loop {
    let ent = p.enter();
    f(p);
    if p.at(sep) {
      p.bump();
      p.exit(ent, wrap);
    } else {
      p.exit(ent, wrap);
      break;
    }
  }
}

pub(crate) fn path(p: &mut Parser<'_, SK>) -> Exited {
  let e = p.enter();
  p.eat(SK::Name);
  loop {
    if p.at(SK::Dot) {
      p.bump();
      p.eat(SK::Name);
    } else {
      break;
    }
  }
  p.exit(e, SK::Path)
}

pub(crate) fn scon(p: &mut Parser<'_, SK>) -> bool {
  if p.at(SK::IntLit)
    || p.at(SK::RealLit)
    || p.at(SK::WordLit)
    || p.at(SK::CharLit)
    || p.at(SK::StringLit)
  {
    p.bump();
    true
  } else {
    false
  }
}
