//! molde — agent-first static framework.
//! A site is data: routes.yaml + templates + a data snapshot.
//! A build is a pure function: (templates, snapshot) -> deterministic bytes.
//!
//! molde is a command-line tool. The Rust API is not stable in 0.x and may change in any
//! release.

// Publishing swaps `current` with a symlink rename, which molde implements for Unix only.
#[cfg(not(unix))]
compile_error!(
    "molde 0.1 supports Linux and macOS only: publishing relies on Unix symlinks. \
     On Windows, use WSL."
);

mod build;
mod check;
mod diff;
pub mod events;
mod http;
mod i18n;
mod locales;
pub mod mcp;
mod render;
mod routes;
mod serve;
mod site;
mod store;
mod verify;

pub use build::{build, BuildReport};
pub use check::run_check;
pub use diff::diff_builds;
pub use render::render_url;
pub use serve::{serve, WatchOpts};
pub use store::gc_builds;
pub use verify::{run_verify, VerifyOptions};

/// Internals the integration tests reach. Not part of the API: no stability promise.
#[doc(hidden)]
pub mod testing {
    pub use crate::build::render_site;
    pub use crate::check::budgets::glob_regex;
    pub use crate::check::head::check_head;
    pub use crate::check::links::{check_links, check_links_for};
    pub use crate::check::paths::check_duplicate_paths;
    pub use crate::check::sitemap::{check_sitemap, check_sitemap_at};
    pub use crate::check::titles::check_titles;
    pub use crate::render::{engine, page_url, render_page};
    pub use crate::routes::{expand_routes, is_valid_lastmod};
    pub use crate::serve::{serve_response, serve_watching, site_fingerprint};
    pub use crate::site::load_site;
    pub use crate::store::{list_builds, lock_out, publish};

    pub mod i18n {
        pub use crate::i18n::*;
    }
}
