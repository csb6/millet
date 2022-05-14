use crate::util::Cx;
use syntax::ast;

pub(crate) fn get(cx: &mut Cx, pat: Option<ast::Pat>) -> hir::PatIdx {
  todo!()
}

pub(crate) fn name(s: &str) -> hir::Pat {
  hir::Pat::Con(hir::Path::new(vec![hir::Name::new(s)]), None)
}
