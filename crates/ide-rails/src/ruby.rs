//! The little Ruby the routing DSL is written in: method calls with their
//! positional arguments, keyword options and `do … end` block, and literal
//! values (strings, symbols, arrays, constants). Anything else is
//! [`Value::Unsupported`], which the mapper turns into a diagnostic.

use std::fmt;
use tree_sitter::{Language, Node, Parser, Tree};

pub fn parse(text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&Language::from(tree_sitter_ruby::LANGUAGE)).ok()?;
    parser.parse(text, None)
}

pub fn text<'a>(node: Node<'_>, src: &'a str) -> &'a str {
    node.utf8_text(src.as_bytes()).unwrap_or("")
}

/// 1-based line of a node.
pub fn line(node: Node<'_>) -> u32 {
    node.start_position().row as u32 + 1
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Sym(String),
    Bool(bool),
    Nil,
    Int(i64),
    List(Vec<Value>),
    /// `Admin::PhotosController`, `Sidekiq::Web`.
    Const(String),
    /// `redirect("/x")` / `redirect { … }`: the literal target when there is one.
    Redirect(Option<String>),
    /// What static reading cannot evaluate (interpolation, variables, method calls…).
    Unsupported(&'static str),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Sym(s) => Some(s),
            _ => None,
        }
    }

    /// `:a` → `[a]`, `[:a, :b]` → `[a, b]`; `None` when any element is not a string / symbol.
    pub fn as_names(&self) -> Option<Vec<String>> {
        match self {
            Value::Str(s) | Value::Sym(s) => Some(vec![s.clone()]),
            Value::List(items) => items.iter().map(|i| i.as_str().map(str::to_owned)).collect(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Nil => Some(false),
            _ => None,
        }
    }

    pub fn is_falsy(&self) -> bool {
        matches!(self, Value::Bool(false) | Value::Nil)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "\"{s}\""),
            Value::Sym(s) => write!(f, ":{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Nil => f.write_str("nil"),
            Value::Int(i) => write!(f, "{i}"),
            Value::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Const(c) => f.write_str(c),
            Value::Redirect(Some(t)) => write!(f, "redirect(\"{t}\")"),
            Value::Redirect(None) => f.write_str("redirect(…)"),
            Value::Unsupported(what) => f.write_str(what),
        }
    }
}

/// A string literal without interpolation or escapes; `None` otherwise.
fn plain_string(node: Node<'_>, src: &str) -> Option<String> {
    let mut out = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_content" => out.push_str(text(child, src)),
            "escape_sequence" => {
                let esc = text(child, src);
                match esc {
                    "\\n" => out.push('\n'),
                    "\\t" => out.push('\t'),
                    "\\\\" => out.push('\\'),
                    "\\\"" => out.push('"'),
                    "\\'" => out.push('\''),
                    "\\/" => out.push('/'),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

pub fn value(node: Node<'_>, src: &str) -> Value {
    match node.kind() {
        "string" => plain_string(node, src).map_or(Value::Unsupported("interpolated string"), Value::Str),
        "bare_string" => plain_string(node, src).map_or(Value::Unsupported("interpolated string"), Value::Str),
        "simple_symbol" => Value::Sym(text(node, src).trim_start_matches(':').to_owned()),
        "hash_key_symbol" => Value::Sym(text(node, src).to_owned()),
        "delimited_symbol" | "bare_symbol" => {
            plain_string(node, src).map_or(Value::Unsupported("interpolated symbol"), Value::Sym)
        }
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "nil" => Value::Nil,
        "integer" => text(node, src).replace('_', "").parse().map_or(Value::Unsupported("integer"), Value::Int),
        "array" | "symbol_array" | "string_array" => {
            let mut cursor = node.walk();
            Value::List(
                node.named_children(&mut cursor).filter(|c| c.kind() != "comment").map(|c| value(c, src)).collect(),
            )
        }
        "constant" | "scope_resolution" => Value::Const(text(node, src).to_owned()),
        "parenthesized_statements" => {
            let mut cursor = node.walk();
            let mut inner = node.named_children(&mut cursor).filter(|c| c.kind() != "comment");
            match (inner.next(), inner.next()) {
                (Some(only), None) => value(only, src),
                _ => Value::Unsupported("expression"),
            }
        }
        "call" => {
            let method = node.child_by_field_name("method").map(|m| text(m, src)).unwrap_or("");
            if method == "redirect" && node.child_by_field_name("receiver").is_none() {
                let target = node
                    .child_by_field_name("arguments")
                    .and_then(|a| a.named_child(0))
                    .map(|a| value(a, src))
                    .and_then(|v| v.as_str().map(str::to_owned));
                Value::Redirect(target)
            } else {
                Value::Unsupported("method call")
            }
        }
        "identifier" => Value::Unsupported("variable"),
        "lambda" => Value::Unsupported("lambda"),
        "regex" => Value::Unsupported("regular expression"),
        "hash" => Value::Unsupported("hash"),
        _ => Value::Unsupported("expression"),
    }
}

/// A keyword / hash option of a call: `to: "x#y"` or `"photos" => "photos#index"`.
#[derive(Clone, Debug)]
pub struct Option_ {
    pub key: Value,
    pub value: Value,
}

/// The arguments of a DSL call, split into positional values and options.
#[derive(Clone, Debug, Default)]
pub struct Args {
    pub positional: Vec<Value>,
    pub options: Vec<Option_>,
}

impl Args {
    pub fn parse(list: Option<Node<'_>>, src: &str) -> Args {
        let mut args = Args::default();
        let Some(list) = list else { return args };
        let mut cursor = list.walk();
        for child in list.named_children(&mut cursor) {
            match child.kind() {
                "comment" => {}
                "pair" => args.push_pair(child, src),
                "hash" => {
                    let mut inner = child.walk();
                    for pair in child.named_children(&mut inner) {
                        if pair.kind() == "pair" {
                            args.push_pair(pair, src);
                        } else if pair.kind() != "comment" {
                            args.positional.push(Value::Unsupported("hash splat"));
                        }
                    }
                }
                "splat_argument" | "hash_splat_argument" | "block_argument" | "forward_argument" => {
                    args.positional.push(Value::Unsupported("splat argument"));
                }
                _ => args.positional.push(value(child, src)),
            }
        }
        args
    }

    fn push_pair(&mut self, pair: Node<'_>, src: &str) {
        let key = pair.child_by_field_name("key").map_or(Value::Unsupported("hash key"), |k| value(k, src));
        let val = pair.child_by_field_name("value").map_or(Value::Unsupported("hash value"), |v| value(v, src));
        self.options.push(Option_ { key, value: val });
    }

    /// The value of keyword option `name`.
    pub fn option(&self, name: &str) -> Option<&Value> {
        self.options.iter().find(|o| matches!(&o.key, Value::Sym(k) if k == name)).map(|o| &o.value)
    }

    pub fn has_option(&self, name: &str) -> bool {
        self.option(name).is_some()
    }

    /// The first `"path" => target` option (a string key).
    pub fn string_keyed(&self) -> Option<(&str, &Value)> {
        self.options.iter().find_map(|o| match &o.key {
            Value::Str(k) => Some((k.as_str(), &o.value)),
            _ => None,
        })
    }
}

/// The statements of a `do … end` / `{ … }` block, comments excluded.
pub fn block_statements<'t>(block: Node<'t>) -> Vec<Node<'t>> {
    let Some(body) = block.child_by_field_name("body") else { return Vec::new() };
    statements(body)
}

/// The statements of a `program`, `body_statement` or `block_body`.
pub fn statements<'t>(body: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor).filter(|c| c.kind() != "comment").collect()
}

/// Every `call` of method `name` in `node`, in source order.
pub fn calls_named<'t>(node: Node<'t>, name: &str, src: &str, out: &mut Vec<Node<'t>>) {
    let mut cursor = node.walk();
    if node.kind() == "call"
        && node.child_by_field_name("method").is_some_and(|m| text(m, src) == name)
        && node.child_by_field_name("receiver").is_none()
    {
        out.push(node);
    }
    for child in node.named_children(&mut cursor) {
        calls_named(child, name, src, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_call(src: &str) -> (Tree, String) {
        (parse(src).unwrap(), src.to_owned())
    }

    #[test]
    fn splits_positional_and_options() {
        let (tree, src) = first_call(r#"get "photos/:id", to: "photos#show", as: :photo, via: [:get, :post]"#);
        let call = tree.root_node().named_child(0).unwrap();
        assert_eq!(call.kind(), "call");
        let args = Args::parse(call.child_by_field_name("arguments"), &src);
        assert_eq!(args.positional, vec![Value::Str("photos/:id".into())]);
        assert_eq!(args.option("to"), Some(&Value::Str("photos#show".into())));
        assert_eq!(args.option("as"), Some(&Value::Sym("photo".into())));
        assert_eq!(args.option("via").unwrap().as_names().unwrap(), vec!["get", "post"]);
    }

    #[test]
    fn string_keyed_option_and_unsupported_values() {
        let (tree, src) = first_call(
            r##"match "photos" => "photos#index", via: :get
get "#{prefix}/x", to: "a#b"
mount Sidekiq::Web => "/sidekiq"
get "x", to: redirect("/y")
"##,
        );
        let root = tree.root_node();
        let args = |i: usize| Args::parse(root.named_child(i).unwrap().child_by_field_name("arguments"), &src);
        assert_eq!(args(0).string_keyed(), Some(("photos", &Value::Str("photos#index".into()))));
        assert_eq!(args(1).positional, vec![Value::Unsupported("interpolated string")]);
        let mount = args(2);
        assert_eq!(mount.options[0].key, Value::Const("Sidekiq::Web".into()));
        assert_eq!(args(3).option("to"), Some(&Value::Redirect(Some("/y".into()))));
    }
}
