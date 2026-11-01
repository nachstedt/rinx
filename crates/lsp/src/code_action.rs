//! Quick fixes: what one click changes to make a finding go away.
//!
//! Two fixes, one module each: [`dependency`] adds a missing dependency to a
//! build file, fixing what fails the build (roadmap #21); [`noqa`] writes a
//! `.. noqa:` silencing anything else the editor shows (roadmap #35).
//!
//! Each edit is computed with its action, not on resolve: an editor asks for
//! actions whenever the cursor moves, so the file is read moments before the
//! click, and clients that cannot resolve an action get the edit as well.

mod dependency;
mod noqa;

pub use dependency::dependency_fixes;
pub(crate) use noqa::{NoqaFix, noqa_fixes};
