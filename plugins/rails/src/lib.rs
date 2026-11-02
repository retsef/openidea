//! Ruby on Rails plugin: the application's routes for the Endpoints tool
//! window, read statically by `ide-rails` through the host's `read-file`.
//! Build with `../build.sh rails` (needs a WASI sysroot for tree-sitter's C
//! sources; see plugins/README.md).

wit_bindgen::generate!({
    world: "plugin",
    path: "../../wit",
});

mod endpoints;

use openidea::plugin::host::{self, LogLevel};
use std::path::Path;

struct Rails;

impl Guest for Rails {
    fn activate(ctx: Context) -> Result<(), String> {
        host::log(LogLevel::Info, &format!("rails {} activated", ctx.plugin_version));
        Ok(())
    }

    fn deactivate() {}

    fn run_command(id: String, arg: Option<String>) -> Result<Option<String>, String> {
        match id.as_str() {
            "endpoints" => {
                let root = arg.or_else(host::workspace_root).ok_or("no project is open")?;
                let read = |path: &Path| host::read_file(&path.to_string_lossy()).ok();
                let table = ide_rails::routes_with(Path::new(&root), &read)
                    .ok_or_else(|| format!("{root} has no config/routes.rb"))?;
                let json = serde_json::to_string(&endpoints::table(&table)).map_err(|e| e.to_string())?;
                Ok(Some(json))
            }
            _ => Err(format!("unknown command {id}")),
        }
    }
}

export!(Rails);
