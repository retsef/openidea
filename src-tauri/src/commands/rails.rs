//! Ruby on Rails: the project's routes, read statically from `config/routes.rb` (`ide_rails`).

use crate::state::AppState;
use ide_core::{IdeError, IdeResult};
use ide_rails::RailsRouteTable;
use tauri::{State, Window};

/// The route table of the project's Rails application, or `None` when the project
/// has no `config/routes.rb`. Routes files are read on every call; the UI keeps
/// the table and re-reads when one of `files` changes.
#[tauri::command]
#[specta::specta]
pub async fn rails_routes(window: Window, state: State<'_, AppState>) -> IdeResult<Option<RailsRouteTable>> {
    let root = state.project(window.label())?.root.clone();
    tauri::async_runtime::spawn_blocking(move || ide_rails::routes(&root))
        .await
        .map_err(|e| IdeError::internal(e.to_string()))
}
