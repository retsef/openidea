//! The route table as the Endpoints window's JSON (see `crates/ide-plugin/src/endpoints.rs`).

use ide_rails::{RailsRoute, RailsRouteSource, RailsRouteTable, RailsVerb};
use serde::Serialize;

/// The default `bin/rails server` address.
const HOST: &str = "http://localhost:3000";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub name: &'static str,
    pub value: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub method: &'static str,
    pub path: String,
    pub target: String,
    pub target_class: String,
    pub name: Option<String>,
    pub file: String,
    pub line: u32,
    pub request: Request,
    pub docs: Vec<(&'static str, String)>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub endpoints: Vec<Endpoint>,
    pub diagnostics: Vec<Diagnostic>,
    pub files: Vec<String>,
}

fn verb(v: RailsVerb) -> &'static str {
    match v {
        RailsVerb::Get => "GET",
        RailsVerb::Post => "POST",
        RailsVerb::Put => "PUT",
        RailsVerb::Patch => "PATCH",
        RailsVerb::Delete => "DELETE",
        RailsVerb::Any => "ANY",
    }
}

/// `controller#action`; the engine constant for a mounted engine.
pub fn target(r: &RailsRoute) -> String {
    if r.action.is_empty() { r.controller.clone() } else { format!("{}#{}", r.controller, r.action) }
}

/// `admin/photos` → `Admin::PhotosController`; engines and redirects as they are.
pub fn controller_class(r: &RailsRoute) -> String {
    if r.action.is_empty() || r.controller.chars().any(|c| c.is_ascii_uppercase() || c == ':' || c == '(') {
        return r.controller.clone();
    }
    let camel: Vec<String> = r
        .controller
        .split('/')
        .map(|part| {
            part.split('_')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<String>()
        })
        .collect();
    format!("{}Controller", camel.join("::"))
}

/// `/photos/:id` → `/photos/{{id}}`, `*path` → `{{path}}`.
pub fn path_variables(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if (c == ':' || c == '*') && chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || *n == '_') {
            out.push_str("{{");
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    out.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push_str("}}");
        } else {
            out.push(c);
        }
    }
    out
}

/// The `.http` request of a route; `ANY` (match via: :all, engines) is sent as GET.
pub fn request(r: &RailsRoute) -> Request {
    let method = match r.verb {
        RailsVerb::Any => "GET",
        v => verb(v),
    };
    let with_body = matches!(method, "POST" | "PUT" | "PATCH");
    let mut headers = vec![Header { name: "Accept", value: "application/json" }];
    if with_body {
        headers.push(Header { name: "Content-Type", value: "application/json" });
    }
    Request {
        method: method.to_owned(),
        url: format!("{HOST}{}", path_variables(&r.path)),
        headers,
        body: with_body.then_some("{}"),
    }
}

pub fn endpoint(r: &RailsRoute) -> Endpoint {
    let source = match r.source {
        RailsRouteSource::Explicit => "route",
        RailsRouteSource::Resources => "resources",
        RailsRouteSource::Concern => "concern",
    };
    Endpoint {
        method: verb(r.verb),
        path: r.path.clone(),
        target: target(r),
        target_class: controller_class(r),
        name: r.name.clone(),
        file: r.file.to_string_lossy().into_owned(),
        line: r.line,
        request: request(r),
        docs: vec![
            ("Controller", controller_class(r)),
            ("Action", if r.action.is_empty() { "—".to_owned() } else { r.action.clone() }),
            ("Helper", r.name.as_ref().map_or("—".to_owned(), |n| format!("{n}_path / {n}_url"))),
            ("Defined by", source.to_owned()),
        ],
    }
}

pub fn table(t: &RailsRouteTable) -> Table {
    Table {
        endpoints: t.routes.iter().map(endpoint).collect(),
        diagnostics: t
            .diagnostics
            .iter()
            .map(|d| Diagnostic { file: d.file.to_string_lossy().into_owned(), line: d.line, message: d.message.clone() })
            .collect(),
        files: t.files.iter().map(|f| f.to_string_lossy().into_owned()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn route(verb: RailsVerb, path: &str, controller: &str, action: &str) -> RailsRoute {
        RailsRoute {
            verb,
            path: path.into(),
            controller: controller.into(),
            action: action.into(),
            name: Some("photo".into()),
            file: PathBuf::from("/app/config/routes.rb"),
            line: 4,
            source: RailsRouteSource::Resources,
        }
    }

    #[test]
    fn maps_routes() {
        let e = endpoint(&route(RailsVerb::Get, "/admin/photos/:id", "admin/photos", "show"));
        assert_eq!(e.target_class, "Admin::PhotosController");
        assert_eq!(e.request.url, "http://localhost:3000/admin/photos/{{id}}");
        assert_eq!(controller_class(&route(RailsVerb::Any, "/x", "Sidekiq::Web", "")), "Sidekiq::Web");
        assert_eq!(controller_class(&route(RailsVerb::Get, "/x", "blog/photo_albums", "show")), "Blog::PhotoAlbumsController");
        assert_eq!(path_variables("/legacy/*path/:id"), "/legacy/{{path}}/{{id}}");
        let post = request(&route(RailsVerb::Post, "/photos", "photos", "create"));
        assert_eq!(post.body, Some("{}"));
        assert_eq!(request(&route(RailsVerb::Any, "/sidekiq", "Sidekiq::Web", "")).method, "GET");
    }
}
