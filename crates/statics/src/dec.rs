use crate::error::Error;
use crate::pat_match::Pat;
use crate::st::St;
use crate::types::{prepare_generalize, Cx, Env, MetaTyVar, Subst, Ty, ValEnv};
use crate::unify::unify;
use crate::util::apply;
use crate::{exp, pat};
use std::collections::BTreeSet;

pub(crate) fn get(st: &mut St, cx: &Cx, ars: &hir::Arenas, env: &mut Env, dec: hir::DecIdx) {
  match &ars.dec[dec] {
    hir::Dec::Val(_, val_binds) => {
      // we actually resort to indexing logic because this is a little weird:
      // - we represent the recursive nature of ValBinds (and all other things that recurse with
      //   `and`) as a sequence of non-recursive items.
      // - if a ValBind is `rec`, it's not just this one, it's all the rest of them.
      // - we need to go over the recursive ValBinds twice.
      let mut idx = 0usize;
      let mut ve = ValEnv::default();
      while let Some(val_bind) = val_binds.get(idx) {
        if val_bind.rec {
          // this and all other remaining ones are recursive.
          break;
        }
        idx += 1;
        let (pm_pat, want) = pat::get(st, cx, ars, &mut ve, val_bind.pat);
        get_val_exp(st, cx, ars, val_bind.exp, pm_pat, want);
      }
      // deal with the recursive ones. first do all the patterns so we can update the val env. we
      // also need a separate recursive-only val env.
      let mut rec_ve = ValEnv::default();
      let got_pats: Vec<_> = val_binds[idx..]
        .iter()
        .map(|val_bind| pat::get(st, cx, ars, &mut rec_ve, val_bind.pat))
        .collect();
      // make sure the non-recursive and recursive val envs don't clash.
      for (name, val_info) in rec_ve.iter() {
        if ve.insert(name.clone(), val_info.clone()).is_some() {
          st.err(Error::Redefined);
        }
      }
      let mut cx = cx.clone();
      cx.env.val_env.extend(rec_ve);
      for (val_bind, (pm_pat, want)) in val_binds[idx..].iter().zip(got_pats) {
        if !matches!(ars.exp[val_bind.exp], hir::Exp::Fn(_)) {
          st.err(Error::ValRecExpNotFn);
        }
        get_val_exp(st, &cx, ars, val_bind.exp, pm_pat, want);
      }
      for val_info in ve.values_mut() {
        assert!(val_info.ty_scheme.vars.is_empty());
        let mut to_generalize = BTreeSet::<MetaTyVar>::default();
        let mut ins = |mv| {
          to_generalize.insert(mv);
        };
        meta_vars(st.subst(), &mut ins, &val_info.ty_scheme.ty);
        // TODO remove ty_vars(cx)? what even is that?
        let (ty_vars, subst) = prepare_generalize(to_generalize);
        val_info.ty_scheme.vars = ty_vars;
        apply(&subst, &mut val_info.ty_scheme.ty);
      }
      env.val_env.extend(ve);
    }
    hir::Dec::Ty(_) => todo!(),
    hir::Dec::Datatype(_) => todo!(),
    hir::Dec::DatatypeCopy(_, _) => todo!(),
    hir::Dec::Abstype(_, _) => todo!(),
    hir::Dec::Exception(_) => todo!(),
    hir::Dec::Local(_, _) => todo!(),
    hir::Dec::Open(_) => todo!(),
    hir::Dec::Seq(decs) => {
      for &dec in decs {
        get(st, cx, ars, env, dec);
      }
    }
  }
}

fn get_val_exp(
  st: &mut St,
  cx: &Cx,
  ars: &hir::Arenas,
  exp: hir::ExpIdx,
  pm_pat: Pat,
  mut want: Ty,
) {
  let got = exp::get(st, cx, ars, exp);
  unify(st, want.clone(), got);
  apply(st.subst(), &mut want);
  pat::get_match(st, vec![pm_pat], want, Some(Error::NonExhaustiveBinding));
}

/// calls `f` for every MetaTyVar not bound by the `subst` in `ty`.
fn meta_vars<F>(subst: &Subst, f: &mut F, ty: &Ty)
where
  F: FnMut(MetaTyVar),
{
  match ty {
    Ty::None | Ty::BoundVar(_) => {}
    Ty::MetaVar(mv) => match subst.get(mv) {
      None => f(mv.clone()),
      Some(ty) => meta_vars(subst, f, ty),
    },
    Ty::Record(rows) => {
      for ty in rows.values() {
        meta_vars(subst, f, ty);
      }
    }
    Ty::Con(args, _) => {
      for ty in args.iter() {
        meta_vars(subst, f, ty);
      }
    }
    Ty::Fn(param, res) => {
      meta_vars(subst, f, param);
      meta_vars(subst, f, res);
    }
  }
}
