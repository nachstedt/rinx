//! Page-level rendering: everything that turns rendered body HTML into a
//! finished file.
//!
//! [`layout`] owns the `MiniJinja` template rendering and the CSS path
//! computation, delegating the sidebar's `.rst`-to-`.html` href rewriting to
//! [`nav_hrefs`]; [`genindex`] renders the standalone general-index page from
//! the project index. Both read site metadata from [`crate::config`].

mod genindex;
mod layout;
mod nav_hrefs;

pub use genindex::render_genindex;
pub use layout::{PageMeta, css_relative_path, render_page};
