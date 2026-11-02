//! Ruby on Rails support: the application's HTTP routes, read statically from
//! `config/routes.rb` (plus the files it `draw`s) without booting the app.
//!
//! [`routes`] evaluates the routing DSL the way `ActionDispatch::Routing::Mapper`
//! does — `resources`, nesting, `namespace` / `scope`, `member` / `collection`,
//! concerns, `draw`, `mount` — into a flat [`RailsRouteTable`], the table
//! `bin/rails routes` prints. What cannot be evaluated statically (interpolated
//! strings, variables, `direct` / `resolve`, routes defined in Ruby code) is
//! skipped with a [`RailsRouteDiagnostic`], never a failure of the whole table.
//! Inflections are ActiveSupport's defaults ([`inflector`]); custom inflections
//! and `api_only` are not read.

pub mod inflector;
mod mapper;
mod ruby;

use ide_core::IpcPath;
use std::path::Path;

pub use mapper::{RailsRoute, RailsRouteDiagnostic, RailsRouteSource, RailsRouteTable, RailsVerb};

/// The routes file every Rails application has.
pub const ROUTES_FILE: &str = "config/routes.rb";

/// A project "has Rails" when `config/routes.rb` exists.
pub fn is_rails_project(root: &Path) -> bool {
    root.join(ROUTES_FILE).is_file()
}

/// The route table of the Rails application at `root`; `None` when it has no
/// `config/routes.rb`.
pub fn routes(root: &Path) -> Option<RailsRouteTable> {
    is_rails_project(root).then(|| mapper::Reader::new(root).read())
}

/// Shared by the table's `files` and the frontend: the path of a routes file.
pub fn ipc_path(path: &Path) -> IpcPath {
    IpcPath(path.to_path_buf())
}

#[cfg(test)]
mod tests;
