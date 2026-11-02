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
//! and `api_only` are not read. The crate depends on nothing of the IDE, so the
//! Rails plugin (`plugins/rails`) can compile it to WebAssembly.

pub mod inflector;
mod mapper;
mod ruby;

use std::path::Path;

pub use mapper::{RailsRoute, RailsRouteDiagnostic, RailsRouteSource, RailsRouteTable, RailsVerb, ReadFile};

/// The routes file every Rails application has.
pub const ROUTES_FILE: &str = "config/routes.rb";

/// A project "has Rails" when `config/routes.rb` exists.
pub fn is_rails_project(root: &Path) -> bool {
    root.join(ROUTES_FILE).is_file()
}

/// The route table of the Rails application at `root`, read from disk; `None`
/// when it has no `config/routes.rb`.
pub fn routes(root: &Path) -> Option<RailsRouteTable> {
    let read = |path: &Path| std::fs::read_to_string(path).ok();
    is_rails_project(root).then(|| mapper::Reader::new(root, &read).read())
}

/// The route table read through `read_file` (a sandboxed plugin reads through
/// its host). `None` when `config/routes.rb` cannot be read.
pub fn routes_with(root: &Path, read_file: &ReadFile<'_>) -> Option<RailsRouteTable> {
    read_file(&root.join(ROUTES_FILE))?;
    Some(mapper::Reader::new(root, read_file).read())
}

#[cfg(test)]
mod tests;
