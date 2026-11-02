//! The ActiveSupport inflections `resources` relies on: `pluralize` /
//! `singularize` with the default English rules, irregulars and uncountables
//! (`config/initializers/inflections.rb` additions are not read), and
//! `underscore` for engine class names (`Sidekiq::Web` → `sidekiq/web`).

use regex_lite::Regex;
use std::sync::LazyLock;

/// `(pattern, replacement)`, applied in order: the first match wins, as in
/// ActiveSupport where the last defined rule is tried first.
type Rules = Vec<(Regex, &'static str)>;

const UNCOUNTABLE: &[&str] =
    &["equipment", "information", "rice", "money", "species", "series", "fish", "sheep", "jeans", "police"];

/// `(singular, plural)`.
const IRREGULAR: &[(&str, &str)] = &[
    ("person", "people"),
    ("man", "men"),
    ("child", "children"),
    ("sex", "sexes"),
    ("move", "moves"),
    ("zombie", "zombies"),
];

/// ActiveSupport's rules, newest first (the order they are tried in).
const PLURAL: &[(&str, &str)] = &[
    (r"(?i)(quiz)$", "${1}zes"),
    (r"(?i)^(oxen)$", "${1}"),
    (r"(?i)^(ox)$", "${1}en"),
    (r"(?i)^(m|l)ice$", "${1}ice"),
    (r"(?i)^(m|l)ouse$", "${1}ice"),
    (r"(?i)(matr|vert|ind)(?:ix|ex)$", "${1}ices"),
    (r"(?i)(x|ch|ss|sh)$", "${1}es"),
    (r"(?i)([^aeiouy]|qu)y$", "${1}ies"),
    (r"(?i)(hive)$", "${1}s"),
    (r"(?i)(?:([^f])fe|([lr])f)$", "${1}${2}ves"),
    (r"(?i)sis$", "ses"),
    (r"(?i)([ti])a$", "${1}a"),
    (r"(?i)([ti])um$", "${1}a"),
    (r"(?i)(buffal|tomat)o$", "${1}oes"),
    (r"(?i)(bu)s$", "${1}ses"),
    (r"(?i)(alias|status)$", "${1}es"),
    (r"(?i)(octop|vir)i$", "${1}i"),
    (r"(?i)(octop|vir)us$", "${1}i"),
    (r"(?i)^(ax|test)is$", "${1}es"),
    (r"(?i)s$", "s"),
    (r"$", "s"),
];

const SINGULAR: &[(&str, &str)] = &[
    (r"(?i)(database)s$", "${1}"),
    (r"(?i)(quiz)zes$", "${1}"),
    (r"(?i)(matr)ices$", "${1}ix"),
    (r"(?i)(vert|ind)ices$", "${1}ex"),
    (r"(?i)^(ox)en", "${1}"),
    (r"(?i)(alias|status)(es)?$", "${1}"),
    (r"(?i)(octop|vir)(us|i)$", "${1}us"),
    (r"(?i)^(a)x[ie]s$", "${1}xis"),
    (r"(?i)(cris|test)(is|es)$", "${1}is"),
    (r"(?i)(shoe)s$", "${1}"),
    (r"(?i)(o)es$", "${1}"),
    (r"(?i)(bus)(es)?$", "${1}"),
    (r"(?i)^(m|l)ice$", "${1}ouse"),
    (r"(?i)(x|ch|ss|sh)es$", "${1}"),
    (r"(?i)(m)ovies$", "${1}ovie"),
    (r"(?i)(s)eries$", "${1}eries"),
    (r"(?i)([^aeiouy]|qu)ies$", "${1}y"),
    (r"(?i)([lr])ves$", "${1}f"),
    (r"(?i)(tive)s$", "${1}"),
    (r"(?i)(hive)s$", "${1}"),
    (r"(?i)([^f])ves$", "${1}fe"),
    (r"(?i)(^analy)(sis|ses)$", "${1}sis"),
    (r"(?i)((a)naly|(b)a|(d)iagno|(p)arenthe|(p)rogno|(s)ynop|(t)he)(sis|ses)$", "${1}sis"),
    (r"(?i)([ti])a$", "${1}um"),
    (r"(?i)(n)ews$", "${1}ews"),
    (r"(?i)(ss)$", "${1}"),
    (r"(?i)s$", ""),
];

fn compile(rules: &[(&'static str, &'static str)]) -> Rules {
    rules.iter().map(|(p, r)| (Regex::new(p).expect("inflection rule"), *r)).collect()
}

static PLURAL_RULES: LazyLock<Rules> = LazyLock::new(|| compile(PLURAL));
static SINGULAR_RULES: LazyLock<Rules> = LazyLock::new(|| compile(SINGULAR));

/// The last word of `word` (after `_`), which carries the inflection.
fn last_word(word: &str) -> &str {
    word.rsplit('_').next().unwrap_or(word)
}

fn uncountable(word: &str) -> bool {
    UNCOUNTABLE.contains(&last_word(word).to_ascii_lowercase().as_str())
}

/// Replaces the trailing `from` of `word` with `to`, keeping the first letter's case.
fn irregular(word: &str, from: &str, to: &str) -> Option<String> {
    let lower = word.to_ascii_lowercase();
    if !lower.ends_with(from) {
        return None;
    }
    let start = word.len() - from.len();
    // Only whole words: `man` must not match `human`.
    if start > 0 && word.as_bytes()[start - 1] != b'_' {
        return None;
    }
    let first = word[start..].chars().next()?;
    let replacement = if first.is_ascii_uppercase() {
        let mut r = to.to_owned();
        r[..1].make_ascii_uppercase();
        r
    } else {
        to.to_owned()
    };
    Some(format!("{}{replacement}", &word[..start]))
}

fn apply(rules: &Rules, word: &str) -> String {
    for (re, replacement) in rules {
        if re.is_match(word) {
            return re.replace(word, *replacement).into_owned();
        }
    }
    word.to_owned()
}

pub fn pluralize(word: &str) -> String {
    if word.is_empty() || uncountable(word) {
        return word.to_owned();
    }
    for (singular, plural) in IRREGULAR {
        if irregular(word, plural, plural).is_some() {
            return word.to_owned();
        }
        if let Some(p) = irregular(word, singular, plural) {
            return p;
        }
    }
    apply(&PLURAL_RULES, word)
}

pub fn singularize(word: &str) -> String {
    if word.is_empty() || uncountable(word) {
        return word.to_owned();
    }
    for (singular, plural) in IRREGULAR {
        if irregular(word, singular, singular).is_some() {
            return word.to_owned();
        }
        if let Some(s) = irregular(word, plural, singular) {
            return s;
        }
    }
    apply(&SINGULAR_RULES, word)
}

/// `Sidekiq::Web` → `sidekiq/web`, `PhotoAlbum` → `photo_album`.
pub fn underscore(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.replace("::", "/").chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let boundary = match prev {
                Some(p) if p.is_ascii_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_ascii_uppercase() => next.is_some_and(|n| n.is_ascii_lowercase()),
                _ => false,
            };
            if boundary {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c == '-' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals() {
        for (s, p) in [
            ("photo", "photos"),
            ("comment", "comments"),
            ("category", "categories"),
            ("address", "addresses"),
            ("box", "boxes"),
            ("person", "people"),
            ("child", "children"),
            ("geocoder", "geocoders"),
            ("status", "statuses"),
            ("news", "news"),
            ("series", "series"),
            ("quiz", "quizzes"),
            ("wolf", "wolves"),
            ("knife", "knives"),
            ("photo_album", "photo_albums"),
            ("sales_person", "sales_people"),
            ("day", "days"),
            ("matrix", "matrices"),
        ] {
            assert_eq!(pluralize(s), p, "pluralize({s})");
            assert_eq!(singularize(p), s, "singularize({p})");
        }
        assert_eq!(pluralize("people"), "people");
        assert_eq!(singularize("photo"), "photo");
        assert_eq!(singularize("human"), "human");
        assert_eq!(singularize("movies"), "movie");
        assert_eq!(singularize("analyses"), "analysis");
    }

    #[test]
    fn underscores() {
        assert_eq!(underscore("Sidekiq::Web"), "sidekiq/web");
        assert_eq!(underscore("PhotoAlbum"), "photo_album");
        assert_eq!(underscore("HTMLParser"), "html_parser");
        assert_eq!(underscore("Blorgh::Engine"), "blorgh/engine");
    }
}
