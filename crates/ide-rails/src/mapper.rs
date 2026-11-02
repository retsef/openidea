//! Static evaluation of the routing DSL, following `ActionDispatch::Routing::Mapper`:
//! a stack of scopes (path, module, name prefix, controller, shallow state,
//! the enclosing resource and the scope level) and the `resources` expansion
//! into the canonical routes, with Rails' own helper-name rules.

use crate::inflector::{pluralize, singularize, underscore};
use crate::ruby::{self, Args, Value, line, text};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tree_sitter::{Node, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RailsVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    /// `match … via: :all`, mounted engines.
    Any,
}

impl RailsVerb {
    fn from_name(name: &str) -> Option<RailsVerb> {
        Some(match name {
            "get" => RailsVerb::Get,
            "post" => RailsVerb::Post,
            "put" => RailsVerb::Put,
            "patch" => RailsVerb::Patch,
            "delete" => RailsVerb::Delete,
            "all" => RailsVerb::Any,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RailsRouteSource {
    /// `get` / `post` / `match` / `root` / `mount` written out.
    Explicit,
    /// One of the canonical routes of `resources` / `resource`.
    Resources,
    /// Produced while expanding a routing concern.
    Concern,
}

/// One row of `bin/rails routes`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RailsRoute {
    pub verb: RailsVerb,
    /// `/photos/:id`, `:param` and `*glob` kept, `(.:format)` omitted.
    pub path: String,
    /// `admin/photos`; the engine constant for `mount`.
    pub controller: String,
    /// Empty for `mount`.
    pub action: String,
    /// Helper prefix (`photo` of `photo_path`), where the route has one.
    pub name: Option<String>,
    /// The routes file holding the DSL call that produced the route.
    pub file: PathBuf,
    /// 1-based line of that call.
    pub line: u32,
    pub source: RailsRouteSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RailsRouteDiagnostic {
    pub file: PathBuf,
    /// 1-based.
    pub line: u32,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RailsRouteTable {
    pub routes: Vec<RailsRoute>,
    pub diagnostics: Vec<RailsRouteDiagnostic>,
    /// Every routes file read (`config/routes.rb` and the `draw`n ones): a change
    /// to one of them invalidates the table.
    pub files: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// Scopes

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Level {
    #[default]
    None,
    Resources,
    Resource,
    Nested,
    Collection,
    Member,
    New,
    Root,
}

/// `Resource` / `SingletonResource` of the mapper.
#[derive(Clone, Debug)]
struct Resource {
    name: String,
    path: String,
    controller: String,
    as_: Option<String>,
    param: String,
    shallow: bool,
    singleton: bool,
    actions: Vec<&'static str>,
}

const RESOURCES_ACTIONS: &[&str] = &["index", "create", "new", "show", "update", "destroy", "edit"];
const RESOURCE_ACTIONS: &[&str] = &["show", "create", "update", "destroy", "new", "edit"];
const CANONICAL_ACTIONS: &[&str] = &["index", "create", "new", "show", "update", "destroy"];
/// Options `resources` keeps for itself; every other one wraps it in a `scope`.
const RESOURCE_OPTIONS: &[&str] = &["as", "controller", "path", "only", "except", "param", "concerns"];

impl Resource {
    fn new(name: &str, singleton: bool, shallow: bool, args: &Args) -> Resource {
        let opt = |k: &str| args.option(k).and_then(|v| v.as_str()).map(str::to_owned);
        let as_ = opt("as");
        let defaults = if singleton { RESOURCE_ACTIONS } else { RESOURCES_ACTIONS };
        let actions: Vec<&'static str> = if let Some(only) = args.option("only").and_then(Value::as_names) {
            defaults.iter().copied().filter(|a| only.iter().any(|o| o == a)).collect()
        } else if let Some(except) = args.option("except").and_then(Value::as_names) {
            defaults.iter().copied().filter(|a| !except.iter().any(|o| o == a)).collect()
        } else {
            defaults.to_vec()
        };
        let controller = opt("controller").unwrap_or_else(|| if singleton { pluralize(name) } else { name.to_owned() });
        Resource {
            name: name.to_owned(),
            path: opt("path").unwrap_or_else(|| name.to_owned()),
            controller,
            as_,
            param: opt("param").unwrap_or_else(|| "id".to_owned()),
            shallow,
            singleton,
            actions,
        }
    }

    /// The name helpers are built from (`as:` wins).
    fn helper_name(&self) -> &str {
        self.as_.as_deref().unwrap_or(&self.name)
    }
    fn plural(&self) -> String {
        if self.singleton { pluralize(self.helper_name()) } else { self.helper_name().to_owned() }
    }
    fn singular(&self) -> String {
        if self.singleton { self.helper_name().to_owned() } else { singularize(self.helper_name()) }
    }
    fn member_name(&self) -> String {
        self.singular()
    }
    fn collection_name(&self) -> String {
        if self.singleton {
            return self.singular();
        }
        let (plural, singular) = (self.plural(), self.singular());
        if singular == plural { format!("{plural}_index") } else { plural }
    }
    fn nested_param(&self) -> String {
        format!("{}_{}", self.singular(), self.param)
    }
    fn member_scope(&self) -> String {
        if self.singleton { self.path.clone() } else { format!("{}/:{}", self.path, self.param) }
    }
    fn nested_scope(&self) -> String {
        if self.singleton { self.path.clone() } else { format!("{}/:{}", self.path, self.nested_param()) }
    }
    fn collection_scope(&self) -> &str {
        &self.path
    }
    fn new_scope(&self, new_path: &str) -> String {
        format!("{}/{new_path}", self.path)
    }
    fn has(&self, action: &str) -> bool {
        self.actions.contains(&action)
    }
}

/// One frame of the mapper's scope, with its parent's values merged in.
#[derive(Clone, Debug, Default)]
struct Scope {
    path: Option<String>,
    module: Option<String>,
    as_: Option<String>,
    controller: Option<String>,
    action: Option<String>,
    shallow: bool,
    shallow_path: Option<String>,
    shallow_prefix: Option<String>,
    path_names: HashMap<String, String>,
    level: Level,
    /// `scope_level_resource`.
    resource: Option<Rc<Resource>>,
    /// Resources enclosing this scope, outermost first (`@nesting`).
    nesting: Vec<Rc<Resource>>,
}

/// `Journey::Router::Utils.normalize_path`: leading `/`, no doubled or trailing slashes.
fn normalize_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 1);
    out.push('/');
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        if out.len() > 1 {
            out.push('/');
        }
        out.push_str(segment);
    }
    // `(.:format)` written out is implied in the table.
    out.strip_suffix("(.:format)").map(str::to_owned).unwrap_or(out)
}

fn merge_path(parent: Option<&str>, child: &str) -> String {
    normalize_path(&format!("{}/{child}", parent.unwrap_or("")))
}

fn merge_as(parent: Option<&str>, child: &str) -> String {
    match parent {
        Some(p) => format!("{p}_{child}"),
        None => child.to_owned(),
    }
}

fn merge_module(parent: Option<&str>, child: &str) -> String {
    match parent {
        Some(p) => format!("{p}/{child}"),
        None => child.to_owned(),
    }
}

/// `Mapper.normalize_name`: `photos/show` → `photos_show`.
fn normalize_name(name: &str) -> String {
    normalize_path(name)[1..].replace('/', "_")
}

/// What a `scope(...)` call changes (`Scoping#scope` options).
#[derive(Clone, Debug, Default)]
struct ScopeOptions {
    path: Option<String>,
    module: Option<String>,
    as_: Option<String>,
    controller: Option<String>,
    shallow: Option<bool>,
    shallow_path: Option<String>,
    shallow_prefix: Option<String>,
    path_names: HashMap<String, String>,
}

impl ScopeOptions {
    fn from_args(args: &Args, out: &mut Vec<String>) -> ScopeOptions {
        let mut o = ScopeOptions::default();
        let name = |v: &Value, key: &str, out: &mut Vec<String>| match v.as_str() {
            Some(s) => Some(s.to_owned()),
            None => {
                out.push(format!("`{key}:` must be a string or symbol, found {v}"));
                None
            }
        };
        for opt in &args.options {
            let Value::Sym(key) = &opt.key else { continue };
            match key.as_str() {
                "path" => o.path = name(&opt.value, key, out),
                "module" => o.module = name(&opt.value, key, out),
                "as" => o.as_ = if opt.value.is_falsy() { None } else { name(&opt.value, key, out) },
                "controller" => o.controller = name(&opt.value, key, out),
                "shallow" => o.shallow = opt.value.as_bool(),
                "shallow_path" => o.shallow_path = name(&opt.value, key, out),
                "shallow_prefix" => o.shallow_prefix = name(&opt.value, key, out),
                // `path_names:` is read from the hash literal by `pair_hash`; the rest is not part of the table.
                _ => {}
            }
        }
        o
    }
}

// ---------------------------------------------------------------------------
// Files

struct FileSource {
    path: PathBuf,
    text: String,
    tree: Tree,
}

/// A `concern :name do … end` definition.
#[derive(Clone)]
struct Concern {
    file: Rc<FileSource>,
    /// Byte range of the `concern` call, re-found when expanded.
    range: (usize, usize),
}

/// Reads a UTF-8 file; `None` when it does not exist or cannot be read.
pub type ReadFile<'a> = dyn Fn(&Path) -> Option<String> + 'a;

pub(crate) struct Reader<'a> {
    read_file: &'a ReadFile<'a>,
    root: PathBuf,
    routes_dir: PathBuf,
    files: Vec<Rc<FileSource>>,
    concerns: HashMap<String, Concern>,
    scopes: Vec<Scope>,
    routes: Vec<RailsRoute>,
    diagnostics: Vec<RailsRouteDiagnostic>,
    named: HashSet<String>,
    /// `draw` files being evaluated (cycle guard).
    drawing: Vec<PathBuf>,
    /// Concerns being expanded (cycle guard); non-empty marks routes as [`RailsRouteSource::Concern`].
    expanding: Vec<String>,
    /// Options of the enclosing `with_options` blocks, outermost first.
    with_options: Vec<Vec<ruby::Option_>>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(root: &Path, read_file: &'a ReadFile<'a>) -> Reader<'a> {
        Reader {
            read_file,
            root: root.to_path_buf(),
            routes_dir: root.join("config").join("routes"),
            files: Vec::new(),
            concerns: HashMap::new(),
            scopes: vec![Scope::default()],
            routes: Vec::new(),
            diagnostics: Vec::new(),
            named: HashSet::new(),
            drawing: Vec::new(),
            expanding: Vec::new(),
            with_options: Vec::new(),
        }
    }

    pub(crate) fn read(mut self) -> RailsRouteTable {
        let main = self.root.join(crate::ROUTES_FILE);
        let Some(file) = self.load(&main) else {
            self.diagnostics.push(RailsRouteDiagnostic {
                file: main.clone(),
                line: 1,
                message: "cannot read config/routes.rb".to_owned(),
            });
            return self.finish();
        };
        // Concerns may live in drawn files: know every file before evaluating.
        self.preload_drawn(&file, &mut HashSet::new());
        self.eval_statements(&file, ruby::statements(file.tree.root_node()));
        self.finish()
    }

    fn finish(self) -> RailsRouteTable {
        RailsRouteTable {
            routes: self.routes,
            diagnostics: self.diagnostics,
            files: self.files.iter().map(|f| f.path.clone()).collect(),
        }
    }

    fn load(&mut self, path: &Path) -> Option<Rc<FileSource>> {
        if let Some(f) = self.files.iter().find(|f| f.path == path) {
            return Some(f.clone());
        }
        let text = (self.read_file)(path)?;
        let tree = ruby::parse(&text)?;
        let file = Rc::new(FileSource { path: path.to_path_buf(), text, tree });
        self.files.push(file.clone());
        self.collect_concerns(&file);
        Some(file)
    }

    fn collect_concerns(&mut self, file: &Rc<FileSource>) {
        let mut calls = Vec::new();
        ruby::calls_named(file.tree.root_node(), "concern", &file.text, &mut calls);
        for call in calls {
            let args = Args::parse(call.child_by_field_name("arguments"), &file.text);
            let Some(name) = args.positional.first().and_then(Value::as_str) else {
                self.diag(file, call, "`concern` needs a name".to_owned());
                continue;
            };
            if call.child_by_field_name("block").is_none() {
                let what = args.positional.get(1).map_or("a callable".to_owned(), |v| v.to_string());
                self.diag(
                    file,
                    call,
                    format!("concern :{name} defined by {what} is not read statically; its routes are missing"),
                );
                continue;
            }
            self.concerns
                .insert(name.to_owned(), Concern { file: file.clone(), range: (call.start_byte(), call.end_byte()) });
        }
    }

    fn drawn_path(&self, name: &str) -> PathBuf {
        self.routes_dir.join(format!("{name}.rb"))
    }

    fn preload_drawn(&mut self, file: &Rc<FileSource>, seen: &mut HashSet<PathBuf>) {
        if !seen.insert(file.path.clone()) {
            return;
        }
        let mut calls = Vec::new();
        ruby::calls_named(file.tree.root_node(), "draw", &file.text, &mut calls);
        for call in calls {
            let args = Args::parse(call.child_by_field_name("arguments"), &file.text);
            if let Some(name) = args.positional.first().and_then(Value::as_str) {
                let path = self.drawn_path(name);
                if let Some(drawn) = self.load(&path) {
                    self.preload_drawn(&drawn, seen);
                }
            }
        }
    }

    // ----------------------------------------------------------------------- scope helpers

    fn scope(&self) -> &Scope {
        self.scopes.last().expect("root scope")
    }

    fn parent_resource(&self) -> Option<Rc<Resource>> {
        self.scope().resource.clone()
    }

    fn level(&self) -> Level {
        self.scope().level
    }

    fn resource_scope(&self) -> bool {
        matches!(self.level(), Level::Resources | Level::Resource)
    }

    fn resource_method_scope(&self) -> bool {
        matches!(self.level(), Level::Collection | Level::Member | Level::New)
    }

    fn canonical_action(&self, action: &str) -> bool {
        self.resource_method_scope() && CANONICAL_ACTIONS.contains(&action)
    }

    /// `shallow?`: the enclosing resource is shallow.
    fn shallow(&self) -> bool {
        self.parent_resource().is_some_and(|r| r.shallow)
    }

    fn with<R>(&mut self, scope: Scope, f: impl FnOnce(&mut Self) -> R) -> R {
        self.scopes.push(scope);
        let r = f(self);
        self.scopes.pop();
        r
    }

    fn with_level<R>(&mut self, level: Level, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        s.level = level;
        self.with(s, f)
    }

    /// `path_scope`.
    fn with_path<R>(&mut self, path: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        s.path = Some(merge_path(s.path.as_deref(), path));
        self.with(s, f)
    }

    /// `controller(name) { }`.
    fn with_controller<R>(&mut self, controller: &str, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        s.controller = Some(controller.to_owned());
        self.with(s, f)
    }

    /// `Scoping#scope(options)`.
    fn with_options<R>(&mut self, mut o: ScopeOptions, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        if s.level != Level::Nested {
            if o.shallow_path.is_none() && o.path.is_some() {
                o.shallow_path = o.path.clone();
            }
            if o.shallow_prefix.is_none() && o.as_.is_some() {
                o.shallow_prefix = o.as_.clone();
            }
        }
        if let Some(p) = &o.path {
            s.path = Some(merge_path(s.path.as_deref(), p));
        }
        if let Some(m) = &o.module {
            s.module = Some(merge_module(s.module.as_deref(), m));
        }
        if let Some(a) = &o.as_ {
            s.as_ = Some(merge_as(s.as_.as_deref(), a));
        }
        if let Some(c) = &o.controller {
            s.controller = Some(c.clone());
        }
        if let Some(sh) = o.shallow {
            s.shallow = sh;
        }
        if let Some(p) = &o.shallow_path {
            s.shallow_path = Some(merge_path(s.shallow_path.as_deref(), p));
        }
        if let Some(p) = &o.shallow_prefix {
            s.shallow_prefix = Some(merge_as(s.shallow_prefix.as_deref(), p));
        }
        s.path_names.extend(o.path_names);
        self.with(s, f)
    }

    /// `shallow_scope`: back to the path / prefix outside the nesting.
    fn with_shallow_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        s.path = s.shallow_path.clone();
        s.as_ = s.shallow_prefix.clone();
        self.with(s, f)
    }

    fn with_resource<R>(&mut self, resource: Rc<Resource>, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut s = self.scope().clone();
        s.nesting.push(resource.clone());
        s.resource = Some(resource.clone());
        s.controller = Some(resource.controller.clone());
        self.with(s, f)
    }

    // ----------------------------------------------------------------------- resource scopes

    fn member<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let r = self.parent_resource().expect("member needs a resource");
        self.with_level(Level::Member, |m| {
            if m.shallow() {
                m.with_shallow_scope(|m| m.with_path(&r.member_scope(), f))
            } else {
                m.with_path(&r.member_scope(), f)
            }
        })
    }

    fn collection<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let r = self.parent_resource().expect("collection needs a resource");
        self.with_level(Level::Collection, |m| m.with_path(r.collection_scope(), f))
    }

    fn new_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let r = self.parent_resource().expect("new needs a resource");
        let new_path = self.action_path("new");
        self.with_level(Level::New, |m| m.with_path(&r.new_scope(&new_path), f))
    }

    fn nested<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let r = self.parent_resource().expect("nested needs a resource");
        let options = ScopeOptions { as_: Some(r.member_name()), ..ScopeOptions::default() };
        self.with_level(Level::Nested, |m| {
            if m.shallow() {
                m.with_shallow_scope(|m| m.with_path(&r.nested_scope(), |m| m.with_options(options, f)))
            } else {
                m.with_path(&r.nested_scope(), |m| m.with_options(options, f))
            }
        })
    }

    fn action_path(&self, action: &str) -> String {
        self.scope().path_names.get(action).cloned().unwrap_or_else(|| action.to_owned())
    }

    // ----------------------------------------------------------------------- names

    fn prefix_name_for_action(&self, as_: Option<&str>, action: Option<&str>) -> Option<String> {
        let prefix = match (as_, action) {
            (Some(a), _) => Some(a),
            (None, Some(act)) if !self.canonical_action(act) => Some(act),
            _ => None,
        }?;
        if prefix == "/" || prefix.is_empty() {
            return None;
        }
        Some(normalize_name(&prefix.replace('-', "_")))
    }

    /// `name_for_action`: the helper prefix of a route, or `None` (unnamed, or an
    /// automatic name already taken).
    fn name_for_action(&self, as_: Option<&str>, action: Option<&str>) -> Option<String> {
        let prefix = self.prefix_name_for_action(as_, action);
        let name_prefix = self.scope().as_.clone();
        let (collection_name, member_name) = match self.parent_resource() {
            Some(r) => {
                as_.or(action)?;
                (Some(r.collection_name()), Some(r.member_name()))
            }
            None => (None, None),
        };
        let parts: Vec<Option<String>> = match self.level() {
            Level::Nested => vec![name_prefix, prefix],
            Level::Collection => vec![prefix, name_prefix, collection_name],
            Level::New => vec![prefix, Some("new".to_owned()), name_prefix, member_name],
            Level::Member => vec![prefix, name_prefix, member_name],
            Level::Root => vec![name_prefix, collection_name, prefix],
            _ => vec![name_prefix, member_name, prefix],
        };
        let candidate = parts.into_iter().flatten().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("_");
        if candidate.is_empty() {
            return None;
        }
        if as_.is_none() {
            let valid = candidate.starts_with(|c: char| c == '_' || c.is_ascii_alphabetic());
            if !valid || self.named.contains(&candidate) {
                return None;
            }
        }
        Some(candidate)
    }

    // ----------------------------------------------------------------------- diagnostics

    fn diag(&mut self, file: &Rc<FileSource>, node: Node<'_>, message: String) {
        self.diagnostics.push(RailsRouteDiagnostic { file: file.path.clone(), line: line(node), message });
    }

    // ----------------------------------------------------------------------- evaluation

    fn eval_statements(&mut self, file: &Rc<FileSource>, statements: Vec<Node<'_>>) {
        for node in statements {
            self.eval_statement(file, node);
        }
    }

    /// A statement of a routes file. Routes inside `if Rails.env.development?` and
    /// the like are part of the table (the condition is not evaluated); other
    /// Ruby (assignments, requires) defines no routes.
    fn eval_statement(&mut self, file: &Rc<FileSource>, node: Node<'_>) {
        match node.kind() {
            "call" => self.eval_call(file, node),
            "if" | "unless" | "elsif" => {
                for field in ["consequence", "alternative"] {
                    if let Some(branch) = node.child_by_field_name(field) {
                        self.eval_statement(file, branch);
                    }
                }
            }
            "if_modifier" | "unless_modifier" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.eval_statement(file, body);
                }
            }
            "then" | "else" | "begin" | "parenthesized_statements" => {
                self.eval_statements(file, ruby::statements(node));
            }
            "case" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "when" => {
                            if let Some(body) = child.child_by_field_name("body") {
                                self.eval_statement(file, body);
                            }
                        }
                        "else" => self.eval_statement(file, child),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn eval_block(&mut self, file: &Rc<FileSource>, call: Node<'_>) {
        if let Some(block) = call.child_by_field_name("block") {
            self.eval_statements(file, ruby::block_statements(block));
        }
    }

    fn eval_call(&mut self, file: &Rc<FileSource>, call: Node<'_>) {
        let src = file.text.as_str();
        let method = call.child_by_field_name("method").map(|m| text(m, src)).unwrap_or("");
        if call.child_by_field_name("receiver").is_some() {
            // `Rails.application.routes.draw do … end`; other receiver calls are not DSL.
            if method == "draw" && call.child_by_field_name("block").is_some() {
                self.eval_block(file, call);
            }
            return;
        }
        let mut args = Args::parse(call.child_by_field_name("arguments"), src);
        // `with_options via: :all do … end`: its options apply to every call inside, the call's own winning.
        for defaults in &self.with_options {
            for o in defaults {
                if let Value::Sym(k) = &o.key
                    && !args.has_option(k)
                {
                    args.options.push(o.clone());
                }
            }
        }
        match method {
            "with_options" => {
                let defaults: Vec<ruby::Option_> = args.options.clone();
                self.with_options.push(defaults);
                self.eval_block(file, call);
                self.with_options.pop();
            }
            "get" | "post" | "put" | "patch" | "delete" => {
                let verb = RailsVerb::from_name(method);
                self.map_match(file, call, verb, args);
            }
            "match" => self.map_match(file, call, None, args),
            "root" => self.root(file, call, args),
            "resources" => self.resources(file, call, args, false),
            "resource" => self.resources(file, call, args, true),
            "namespace" => self.namespace(file, call, args),
            "scope" => {
                let mut problems = Vec::new();
                let mut o = ScopeOptions::from_args(&args, &mut problems);
                if !args.positional.is_empty() {
                    let parts: Option<Vec<String>> =
                        args.positional.iter().map(|v| v.as_str().map(str::to_owned)).collect();
                    match parts {
                        Some(parts) => o.path = Some(parts.join("/")),
                        None => problems.push("the `scope` path must be a string or symbol".to_owned()),
                    }
                }
                o.path_names = pair_hash(&args, "path_names", src, call);
                for p in problems {
                    self.diag(file, call, p);
                }
                self.with_options(o, |m| m.eval_block(file, call));
            }
            "controller" => match args.positional.first().and_then(Value::as_str) {
                Some(c) => {
                    let c = c.to_owned();
                    self.with_controller(&c, |m| m.eval_block(file, call));
                }
                None => self.diag(file, call, "`controller` needs a name".to_owned()),
            },
            "constraints" | "defaults" => self.eval_block(file, call),
            "shallow" => {
                let o = ScopeOptions { shallow: Some(true), ..ScopeOptions::default() };
                self.with_options(o, |m| m.eval_block(file, call));
            }
            "member" | "collection" | "new" | "nested" => {
                if !self.resource_scope() {
                    self.diag(file, call, format!("`{method}` can only be used inside a resource(s) scope"));
                    return;
                }
                match method {
                    "member" => self.member(|m| m.eval_block(file, call)),
                    "collection" => self.collection(|m| m.eval_block(file, call)),
                    "new" => self.new_scope(|m| m.eval_block(file, call)),
                    _ => self.nested(|m| m.eval_block(file, call)),
                }
            }
            // Recorded by `collect_concerns`.
            "concern" => {}
            "concerns" => {
                let names: Vec<String> = args.positional.iter().filter_map(|v| v.as_names()).flatten().collect();
                if names.is_empty() {
                    self.diag(file, call, "`concerns` needs concern names".to_owned());
                }
                for name in names {
                    self.expand_concern(file, call, &name);
                }
            }
            "draw" => match args.positional.first().and_then(Value::as_str) {
                Some(name) => {
                    let name = name.to_owned();
                    self.draw(file, call, &name);
                }
                None => self.diag(file, call, "`draw` needs a file name".to_owned()),
            },
            "mount" => self.mount(file, call, args),
            "direct" | "resolve" => {
                self.diag(file, call, format!("`{method}` defines a URL helper in Ruby code; not read statically"));
            }
            _ => {}
        }
    }

    fn draw(&mut self, file: &Rc<FileSource>, call: Node<'_>, name: &str) {
        let path = self.drawn_path(name);
        if self.drawing.contains(&path) || (file.path == path) {
            self.diag(file, call, format!("`draw :{name}` would draw itself"));
            return;
        }
        let Some(drawn) = self.load(&path) else {
            let rel = format!("config/routes/{name}.rb");
            self.diag(file, call, format!("`draw :{name}`: {rel} not found"));
            return;
        };
        self.drawing.push(path);
        self.eval_statements(&drawn, ruby::statements(drawn.tree.root_node()));
        self.drawing.pop();
    }

    fn expand_concern(&mut self, file: &Rc<FileSource>, call: Node<'_>, name: &str) {
        let Some(concern) = self.concerns.get(name).cloned() else {
            // A concern defined by a class was already reported where it is defined.
            let reported =
                self.diagnostics.iter().any(|d| d.message.starts_with(&format!("concern :{name} defined by")));
            if !reported {
                self.diag(file, call, format!("No concern named :{name} was found"));
            }
            return;
        };
        if self.expanding.iter().any(|n| n == name) {
            self.diag(file, call, format!("concern :{name} includes itself"));
            return;
        }
        let root = concern.file.tree.root_node();
        let Some(def) = root.descendant_for_byte_range(concern.range.0, concern.range.1) else { return };
        let def = if def.kind() == "call" { Some(def) } else { def.parent().filter(|p| p.kind() == "call") };
        let Some(def) = def else { return };
        self.expanding.push(name.to_owned());
        self.eval_block(&concern.file, def);
        self.expanding.pop();
    }

    fn source(&self) -> RailsRouteSource {
        if self.expanding.is_empty() { RailsRouteSource::Explicit } else { RailsRouteSource::Concern }
    }

    // ----------------------------------------------------------------------- match

    /// `get` / `post` / … / `match`.
    fn map_match(&mut self, file: &Rc<FileSource>, call: Node<'_>, verb: Option<RailsVerb>, mut args: Args) {
        let via: Vec<RailsVerb> = match verb {
            Some(v) => vec![v],
            None => match args.option("via") {
                Some(v) => match v
                    .as_names()
                    .and_then(|names| names.iter().map(|n| RailsVerb::from_name(n)).collect::<Option<Vec<_>>>())
                {
                    Some(verbs) if !verbs.is_empty() => verbs,
                    _ => {
                        self.diag(file, call, format!("`via:` must be HTTP verbs or :all, found {v}"));
                        return;
                    }
                },
                None => {
                    self.diag(file, call, "`match` needs `via:` (HTTP verbs or :all)".to_owned());
                    return;
                }
            },
        };
        let mut to = args.option("to").cloned();
        let mut controller = args.option("controller").and_then(|v| v.as_str().map(str::to_owned));
        let mut action = args.option("action").and_then(|v| v.as_str().map(str::to_owned));
        let as_: Option<Option<String>> = match args.option("as") {
            None => None,
            Some(v) if v.is_falsy() => Some(None),
            Some(v) => match v.as_str() {
                Some(s) => Some(Some(s.to_owned())),
                None => {
                    self.diag(file, call, format!("`as:` must be a string or symbol, found {v}"));
                    Some(None)
                }
            },
        };
        let option_path = args.option("path").and_then(|v| v.as_str().map(str::to_owned));

        // Paths: positional strings / symbols, else `"path" => target`.
        let mut paths: Vec<Value> = Vec::new();
        for v in std::mem::take(&mut args.positional) {
            match v {
                Value::Str(_) | Value::Sym(_) => paths.push(v),
                Value::List(items) => paths.extend(items),
                other => {
                    self.diag(file, call, format!("route path {other} is not read statically"));
                    return;
                }
            }
        }
        if paths.is_empty()
            && let Some((path, target)) = args.string_keyed()
        {
            paths.push(Value::Str(path.to_owned()));
            match target {
                Value::Sym(a) => action = Some(a.clone()),
                Value::Str(t) if t.contains('#') => to = Some(target.clone()),
                Value::Str(c) => controller = Some(c.clone()),
                other => to = Some(other.clone()),
            }
        }
        if paths.is_empty() {
            self.diag(file, call, "route path not specified".to_owned());
            return;
        }
        if let Some(t) = &to
            && !matches!(t, Value::Str(_) | Value::Redirect(_) | Value::Const(_))
        {
            self.diag(file, call, format!("`to: {t}` is not read statically"));
            return;
        }

        let on = args.option("on").cloned();
        let routes = Routes { via, to, controller, action, as_, option_path, paths, canonical: false };
        match on {
            Some(Value::Sym(on)) => match on.as_str() {
                "member" | "collection" | "new" if self.parent_resource().is_some() => match on.as_str() {
                    "member" => self.member(|m| m.decomposed(file, call, routes)),
                    "collection" => self.collection(|m| m.decomposed(file, call, routes)),
                    _ => self.new_scope(|m| m.decomposed(file, call, routes)),
                },
                "member" | "collection" | "new" => {
                    self.diag(file, call, format!("`on: :{on}` can only be used inside a resource(s) scope"));
                }
                _ => self.diag(file, call, format!("Unknown scope :{on} given to :on")),
            },
            Some(other) => self.diag(file, call, format!("`on:` must be a symbol, found {other}")),
            None => match self.level() {
                Level::Resources => self.nested(|m| m.decomposed(file, call, routes)),
                Level::Resource => self.member(|m| m.decomposed(file, call, routes)),
                _ => self.decomposed(file, call, routes),
            },
        }
    }

    fn decomposed(&mut self, file: &Rc<FileSource>, call: Node<'_>, routes: Routes) {
        for path in &routes.paths {
            match path {
                Value::Str(p) => {
                    // `get "photos/show"` → `photos#show`.
                    let to = routes.to.clone().or_else(|| {
                        if routes.action.is_some() {
                            return None;
                        }
                        shorthand_target(p).map(Value::Str)
                    });
                    self.add_route(file, call, &routes, p, Some(p), to);
                }
                Value::Sym(action) => {
                    let to = routes.to.clone();
                    self.add_route(file, call, &routes, action, routes.option_path.as_deref(), to);
                }
                _ => {}
            }
        }
    }

    fn path_for_action(&self, action: &str, path: Option<&str>) -> String {
        let scope_path = self.scope().path.clone().unwrap_or_default();
        match path {
            Some(p) => format!("{scope_path}/{p}"),
            None if self.canonical_action(action) => scope_path,
            None => format!("{scope_path}/{}", self.action_path(action)),
        }
    }

    /// `Mapping#add_route`: one route per verb.
    fn add_route(
        &mut self,
        file: &Rc<FileSource>,
        call: Node<'_>,
        routes: &Routes,
        action: &str,
        path: Option<&str>,
        to: Option<Value>,
    ) {
        let full_path = normalize_path(&self.path_for_action(action, path));
        let mut default_action = routes.action.clone().or_else(|| self.scope().action.clone());
        let simple = action.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/');
        let action_for_name = if simple {
            if default_action.is_none() && !action.contains('/') {
                default_action = Some(action.replace('-', "_"));
            }
            Some(action)
        } else {
            None
        };
        let mut name = match &routes.as_ {
            Some(None) => None,
            Some(Some(as_)) => self.name_for_action(Some(as_), action_for_name),
            None => self.name_for_action(None, action_for_name),
        };
        if let Some(n) = &name
            && self.named.contains(n)
        {
            self.diag(file, call, format!("Invalid route name, already in use: '{n}'"));
            name = None;
        }

        let (controller, action) = match &to {
            Some(Value::Redirect(target)) => {
                let target = target.as_deref().map_or("redirect(…)".to_owned(), |t| format!("redirect({t})"));
                (target, String::new())
            }
            Some(Value::Const(app)) => (app.clone(), String::new()),
            Some(Value::Str(t)) if t.contains('#') => {
                let (c, a) = t.split_once('#').unwrap_or((t, ""));
                let controller = routes.controller.clone().filter(|_| c.is_empty()).unwrap_or_else(|| c.to_owned());
                let action = if a.is_empty() { default_action.clone() } else { Some(a.to_owned()) };
                (self.with_module(&controller), action.unwrap_or_default())
            }
            Some(Value::Str(_)) | None => {
                let Some(controller) = routes.controller.clone().or_else(|| self.scope().controller.clone()) else {
                    self.diag(
                        file,
                        call,
                        format!("Missing :controller key on routes definition for \"{full_path}\" (use `to:`)"),
                    );
                    return;
                };
                let Some(action) = default_action else {
                    self.diag(file, call, format!("Missing :action key on routes definition for \"{full_path}\""));
                    return;
                };
                (self.with_module(&controller), action)
            }
            Some(_) => return,
        };
        let source = match self.source() {
            RailsRouteSource::Explicit if routes.canonical => RailsRouteSource::Resources,
            s => s,
        };
        if let Some(n) = &name {
            self.named.insert(n.clone());
        }
        for verb in &routes.via {
            self.routes.push(RailsRoute {
                verb: *verb,
                path: full_path.clone(),
                controller: controller.clone(),
                action: action.clone(),
                name: name.clone(),
                file: file.path.clone(),
                line: line(call),
                source,
            });
        }
    }

    /// `add_controller_module`: a leading `/` escapes the module scope.
    fn with_module(&self, controller: &str) -> String {
        if let Some(absolute) = controller.strip_prefix('/') {
            return absolute.to_owned();
        }
        match &self.scope().module {
            Some(m) => format!("{m}/{controller}"),
            None => controller.to_owned(),
        }
    }

    fn root(&mut self, file: &Rc<FileSource>, call: Node<'_>, mut args: Args) {
        if let Some(Value::Str(to)) = args.positional.first().cloned() {
            args.positional.clear();
            args.options.push(ruby::Option_ { key: Value::Sym("to".into()), value: Value::Str(to) });
        }
        if !args.has_option("as") {
            args.options.push(ruby::Option_ { key: Value::Sym("as".into()), value: Value::Sym("root".into()) });
        }
        args.positional = vec![Value::Str("/".into())];
        if self.resource_scope() {
            self.with_level(Level::Root, |m| m.map_match(file, call, Some(RailsVerb::Get), args));
        } else {
            self.map_match(file, call, Some(RailsVerb::Get), args);
        }
    }

    fn mount(&mut self, file: &Rc<FileSource>, call: Node<'_>, args: Args) {
        let (app, path) = match (args.positional.first(), args.option("at"), args.options.first()) {
            (Some(Value::Const(app)), Some(Value::Str(at)), _) => (app.clone(), at.clone()),
            (None, _, Some(o)) if matches!(o.key, Value::Const(_)) => match (&o.key, &o.value) {
                (Value::Const(app), Value::Str(at)) => (app.clone(), at.clone()),
                (_, other) => {
                    self.diag(file, call, format!("`mount` path {other} is not read statically"));
                    return;
                }
            },
            _ => {
                self.diag(file, call, "`mount` needs an engine constant and a path (`at:`)".to_owned());
                return;
            }
        };
        let as_ = match args.option("as") {
            Some(v) if v.is_falsy() => None,
            Some(v) => v.as_str().map(str::to_owned),
            None => Some(underscore(&app).replace('/', "_")),
        };
        let full_path = normalize_path(&format!("{}/{path}", self.scope().path.clone().unwrap_or_default()));
        let name = self.name_for_action(as_.as_deref(), Some(&path));
        if let Some(n) = &name {
            self.named.insert(n.clone());
        }
        self.routes.push(RailsRoute {
            verb: RailsVerb::Any,
            path: full_path,
            controller: app,
            action: String::new(),
            name,
            file: file.path.clone(),
            line: line(call),
            source: self.source(),
        });
    }

    // ----------------------------------------------------------------------- resources

    fn namespace(&mut self, file: &Rc<FileSource>, call: Node<'_>, args: Args) {
        let Some(name) = args.positional.first().and_then(Value::as_str).map(str::to_owned) else {
            self.diag(file, call, "`namespace` needs a name".to_owned());
            return;
        };
        if self.resource_scope() {
            return self.nested(|m| m.namespace_in_scope(file, call, &name, &args));
        }
        self.namespace_in_scope(file, call, &name, &args);
    }

    fn namespace_in_scope(&mut self, file: &Rc<FileSource>, call: Node<'_>, name: &str, args: &Args) {
        let mut problems = Vec::new();
        let mut o = ScopeOptions::from_args(args, &mut problems);
        for p in problems {
            self.diag(file, call, p);
        }
        let path = o.path.take().unwrap_or_else(|| name.to_owned());
        let as_given = args.has_option("as");
        o.module.get_or_insert_with(|| name.to_owned());
        if !as_given {
            o.as_ = Some(name.to_owned());
        }
        o.shallow_path.get_or_insert_with(|| path.clone());
        if o.shallow_prefix.is_none() {
            o.shallow_prefix = o.as_.clone();
        }
        self.with_path(&path, |m| m.with_options(o, |m| m.eval_block(file, call)));
    }

    fn resources(&mut self, file: &Rc<FileSource>, call: Node<'_>, args: Args, singleton: bool) {
        let names: Vec<String> = args.positional.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        if names.is_empty() || names.len() != args.positional.len() {
            self.diag(
                file,
                call,
                format!("`{}` needs resource names", if singleton { "resource" } else { "resources" }),
            );
            return;
        }
        for name in names {
            self.one_resource(file, call, &name, &args, singleton);
        }
    }

    /// `apply_common_behavior_for` then the resource itself.
    fn one_resource(&mut self, file: &Rc<FileSource>, call: Node<'_>, name: &str, args: &Args, singleton: bool) {
        if args.option("shallow").is_some_and(|v| v.as_bool() == Some(true)) {
            let mut inner = args.clone();
            inner.options.retain(|o| !matches!(&o.key, Value::Sym(k) if k == "shallow"));
            let o = ScopeOptions { shallow: Some(true), ..ScopeOptions::default() };
            return self.with_options(o, |m| m.one_resource(file, call, name, &inner, singleton));
        }
        if self.resource_scope() {
            return self.nested(|m| m.one_resource_at_level(file, call, name, args, singleton));
        }
        self.one_resource_at_level(file, call, name, args, singleton);
    }

    fn one_resource_at_level(
        &mut self,
        file: &Rc<FileSource>,
        call: Node<'_>,
        name: &str,
        args: &Args,
        singleton: bool,
    ) {
        // Options `resources` does not own become a surrounding `scope`.
        let mut scope_args = Args::default();
        let mut own = Args { positional: Vec::new(), options: Vec::new() };
        for o in &args.options {
            match &o.key {
                Value::Sym(k) if RESOURCE_OPTIONS.contains(&k.as_str()) => own.options.push(o.clone()),
                Value::Sym(k) if k == "shallow" && o.value.is_falsy() => scope_args.options.push(o.clone()),
                Value::Sym(k) if matches!(k.as_str(), "constraints" | "defaults" | "format") => {}
                _ => scope_args.options.push(o.clone()),
            }
        }
        if !scope_args.options.is_empty() {
            let mut problems = Vec::new();
            let mut o = ScopeOptions::from_args(&scope_args, &mut problems);
            o.path_names = pair_hash(&scope_args, "path_names", &file.text, call);
            for p in problems {
                self.diag(file, call, p);
            }
            return self.with_options(o, |m| m.define_resource(file, call, name, &own, singleton));
        }
        self.define_resource(file, call, name, &own, singleton);
    }

    fn define_resource(&mut self, file: &Rc<FileSource>, call: Node<'_>, name: &str, args: &Args, singleton: bool) {
        for key in ["as", "controller", "path", "param"] {
            if let Some(v) = args.option(key)
                && v.as_str().is_none()
            {
                self.diag(file, call, format!("`{key}:` must be a string or symbol, found {v}"));
                return;
            }
        }
        for key in ["only", "except"] {
            if let Some(v) = args.option(key)
                && v.as_names().is_none()
            {
                self.diag(file, call, format!("`{key}:` must be action names, found {v}"));
                return;
            }
        }
        let shallow = self.scope().shallow;
        let resource = Rc::new(Resource::new(name, singleton, shallow, args));
        let concerns: Vec<String> = args.option("concerns").and_then(Value::as_names).unwrap_or_default();
        let level = if singleton { Level::Resource } else { Level::Resources };
        self.with_level(level, |m| {
            m.with_resource(resource.clone(), |m| {
                m.eval_block(file, call);
                for c in &concerns {
                    m.expand_concern(file, call, c);
                }
                let r = &resource;
                if singleton {
                    if r.has("new") {
                        m.new_scope(|m| m.canonical(file, call, RailsVerb::Get, "new"));
                    }
                    m.member_mappings(file, call, r);
                    if r.has("create") {
                        m.collection(|m| m.canonical(file, call, RailsVerb::Post, "create"));
                    }
                } else {
                    m.collection(|m| {
                        if r.has("index") {
                            m.canonical(file, call, RailsVerb::Get, "index");
                        }
                        if r.has("create") {
                            m.canonical(file, call, RailsVerb::Post, "create");
                        }
                    });
                    if r.has("new") {
                        m.new_scope(|m| m.canonical(file, call, RailsVerb::Get, "new"));
                    }
                    m.member_mappings(file, call, r);
                }
            })
        });
    }

    /// `set_member_mappings_for_resource`.
    fn member_mappings(&mut self, file: &Rc<FileSource>, call: Node<'_>, r: &Resource) {
        self.member(|m| {
            if r.has("edit") {
                m.canonical(file, call, RailsVerb::Get, "edit");
            }
            if r.has("show") {
                m.canonical(file, call, RailsVerb::Get, "show");
            }
            if r.has("update") {
                m.canonical(file, call, RailsVerb::Patch, "update");
                m.canonical(file, call, RailsVerb::Put, "update");
            }
            if r.has("destroy") {
                m.canonical(file, call, RailsVerb::Delete, "destroy");
            }
        });
    }

    /// `get :index` & co. inside the resource's own scopes.
    fn canonical(&mut self, file: &Rc<FileSource>, call: Node<'_>, verb: RailsVerb, action: &str) {
        let routes = Routes {
            via: vec![verb],
            to: None,
            controller: None,
            action: None,
            as_: None,
            option_path: None,
            paths: vec![Value::Sym(action.to_owned())],
            canonical: true,
        };
        self.add_route(file, call, &routes, action, None, None);
    }
}

/// `get_to_from_path`: `photos/show` → `photos#show` when the path has only word segments.
fn shorthand_target(path: &str) -> Option<String> {
    let p = path.strip_prefix('/').unwrap_or(path);
    let ok = p.contains('/')
        && p.split('/').all(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
    if !ok {
        return None;
    }
    let (controller, action) = p.rsplit_once('/')?;
    Some(format!("{controller}#{action}").replace('-', "_"))
}

/// `path_names: { new: "neu", edit: "bearbeiten" }`, read from the call's hash literal.
fn pair_hash(args: &Args, key: &str, src: &str, call: Node<'_>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if !args.has_option(key) {
        return out;
    }
    let Some(list) = call.child_by_field_name("arguments") else { return out };
    let mut cursor = list.walk();
    let pairs: Vec<Node<'_>> = list.named_children(&mut cursor).flat_map(|c| pairs_of(c)).collect();
    for pair in pairs {
        let Some(k) = pair.child_by_field_name("key") else { continue };
        if ruby::value(k, src).as_str() != Some(key) {
            continue;
        }
        let Some(hash) = pair.child_by_field_name("value") else { continue };
        if hash.kind() != "hash" {
            continue;
        }
        let mut inner = hash.walk();
        for p in hash.named_children(&mut inner) {
            if p.kind() != "pair" {
                continue;
            }
            if let (Some(k), Some(v)) = (p.child_by_field_name("key"), p.child_by_field_name("value"))
                && let (Some(k), Some(v)) = (ruby::value(k, src).as_str(), ruby::value(v, src).as_str())
            {
                out.insert(k.to_owned(), v.to_owned());
            }
        }
    }
    out
}

fn pairs_of(node: Node<'_>) -> Vec<Node<'_>> {
    match node.kind() {
        "pair" => vec![node],
        "hash" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).filter(|c| c.kind() == "pair").collect()
        }
        _ => Vec::new(),
    }
}

/// What one `match` call maps, before the per-path decomposition.
struct Routes {
    via: Vec<RailsVerb>,
    to: Option<Value>,
    controller: Option<String>,
    action: Option<String>,
    /// `Some(None)` for `as: nil` / `as: false` (no helper).
    as_: Option<Option<String>>,
    option_path: Option<String>,
    paths: Vec<Value>,
    /// One of the canonical `resources` routes.
    canonical: bool,
}
