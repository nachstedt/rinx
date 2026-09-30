//! Page-level rendering: everything that turns rendered body HTML into a
//! finished file.
//!
//! [`layout`] owns the `MiniJinja` template rendering and the CSS path
//! computation, delegating the sidebar's `.rst`-to-`.html` href rewriting to
//! [`nav_hrefs`]; [`genindex`] and [`modindex`] render the standalone general
//! index and Python Module Index pages from the project index. All of them
//! read site metadata from [`crate::config`].

mod genindex;
mod layout;
mod modindex;

pub use genindex::render_genindex;
pub use layout::{PageMeta, css_relative_path, render_page};
pub use modindex::{MODINDEX_PATH, MODINDEX_TITLE, render_modindex};
