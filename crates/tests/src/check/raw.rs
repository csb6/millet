//! The "raw" test runner. Usually we use various convenient shortcuts on top of this.

use crate::check::{expect, input, reason, show};

/// An expected outcome from a test.
#[derive(Debug)]
pub(crate) enum Outcome {
  Pass,
  Fail,
}

/// The low-level impl that almost all top-level functions delegate to.
pub(crate) fn get<'a, I>(
  files: I,
  std_basis: analysis::StdBasis,
  want: Outcome,
  min_severity: diagnostic_util::Severity,
) where
  I: IntoIterator<Item = (&'a str, &'a str)>,
{
  // ignore the Err if we already initialized logging, since that's fine.
  let (input, store) = input::get(files);
  let input = input.expect("unexpectedly bad input");
  let mut ck = show::Show::new(
    store,
    input.iter_sources().map(|s| {
      let file = expect::File::new(s.val);
      (s.path, file)
    }),
  );
  let want_err_len: usize = ck
    .files
    .values()
    .map(|x| {
      x.iter()
        .filter(|(_, e)| matches!(e.kind, expect::Kind::ErrorExact | expect::Kind::ErrorContains))
        .count()
    })
    .sum();
  // NOTE: we used to emit an error here if want_err_len was not 0 or 1 but no longer. this
  // allows us to write multiple error expectations. e.g. in the diagnostics tests. but note that
  // only one expectation is actually used.
  let mut an = analysis::Analysis::new(
    std_basis,
    config::ErrorLines::One,
    config::DiagnosticsFilter::None,
    false,
    true,
  );
  let err = an
    .get_many(&input)
    .into_iter()
    .flat_map(|(id, errors)| {
      errors.into_iter().filter_map(move |e| (e.severity >= min_severity).then_some((id, e)))
    })
    .next();
  for (&path, file) in &ck.files {
    for (&region, expect) in file.iter() {
      if matches!(expect.kind, expect::Kind::Hover) {
        let pos = match region {
          expect::Region::Exact { line, col_start, .. } => {
            text_pos::Position { line, character: col_start }
          }
          expect::Region::Line(n) => {
            ck.reasons.push(reason::Reason::InexactHover(path.wrap(n)));
            continue;
          }
        };
        let r = match an.get_md(path.wrap(pos), true) {
          None => reason::Reason::NoHover(path.wrap(region)),
          Some((got, _)) => {
            if got.contains(&expect.msg) {
              continue;
            }
            reason::Reason::Mismatched(path.wrap(region), expect.msg.clone(), got)
          }
        };
        ck.reasons.push(r);
      }
    }
  }
  let had_error = match err {
    Some((id, e)) => {
      match reason::get(&ck.files, id, e.range, e.message) {
        Ok(()) => {}
        Err(r) => ck.reasons.push(r),
      }
      true
    }
    None => false,
  };
  if !had_error && want_err_len != 0 {
    ck.reasons.push(reason::Reason::NoErrorsEmitted(want_err_len));
  }
  match (want, ck.reasons.is_empty()) {
    (Outcome::Pass, true) | (Outcome::Fail, false) => {}
    (Outcome::Pass, false) => panic!("UNEXPECTED FAIL: {ck}"),
    (Outcome::Fail, true) => panic!("UNEXPECTED PASS: {ck}"),
  }
}