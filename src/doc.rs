//! The data of a scene: tables of keys and values, grouped in sections. A scene
//! diagram describes them with `%% @<section> key=value ...` lines (see
//! `parse_line`) and with its higher-level directives.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Arr(Vec<Value>),
}

#[derive(Debug, Default, Clone)]
pub struct Table {
    pub name: String,
    pub line: usize,
    /// The file this table comes from (empty: given as text).
    pub file: String,
    pub entries: BTreeMap<String, Value>,
}

#[derive(Debug, Default)]
pub struct Doc {
    /// Sections with one table, such as `video`.
    pub tables: BTreeMap<String, Table>,
    /// Sections with a list of tables, such as `point` or `step`, in order.
    pub arrays: BTreeMap<String, Vec<Table>>,
    /// The files read to build this document (the scene and its includes).
    pub files: Vec<PathBuf>,
}

/// Sections with a single table: later lines add keys to it, the last one wins.
pub const SINGLE: [&str; 1] = ["video"];

impl Doc {
    /// Adds a table: to its list, or merged into a single-table section.
    pub fn add(&mut self, t: Table) {
        if SINGLE.contains(&t.name.as_str()) {
            match self.tables.get_mut(&t.name) {
                Some(d) => d.entries.extend(t.entries),
                None => {
                    self.tables.insert(t.name.clone(), t);
                }
            }
        } else {
            self.arrays.entry(t.name.clone()).or_default().push(t);
        }
    }
}

/// Reads `<section> key=value key="text with spaces" list=a,b,c ...` (the part of
/// an `@` line after the `@`). Values: `true`/`false`, numbers, comma-separated
/// lists, quoted strings (with `\"` and `\\`), or a single word.
pub fn parse_line(text: &str) -> Result<(String, BTreeMap<String, Value>), String> {
    let mut chars = text.trim().chars().peekable();
    let mut section = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            break;
        }
        section.push(c);
        chars.next();
    }
    if section.is_empty() || !section.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("'@{section}': a section name is letters, digits and _"));
    }
    let mut entries = BTreeMap::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if chars.next() != Some('=') {
            return Err(format!("'{key}' needs a value: write {key}=..."));
        }
        let value = if chars.peek() == Some(&'"') {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next() {
                    None => return Err(format!("the value of '{key}' has no closing quote")),
                    Some('"') => break,
                    Some('\\') => s.push(chars.next().ok_or_else(|| format!("the value of '{key}' ends with \\"))?),
                    Some(c) => s.push(c),
                }
            }
            Value::Str(s)
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            if s.is_empty() {
                return Err(format!("'{key}' has an empty value"));
            }
            word(&s)
        };
        if entries.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate key '{key}'"));
        }
    }
    Ok((section, entries))
}

/// An unquoted value.
fn word(s: &str) -> Value {
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ if s.contains(',') => Value::Arr(s.split(',').filter(|p| !p.is_empty()).map(|p| Value::Str(p.to_string())).collect()),
        _ => s.parse::<f64>().map_or_else(|_| Value::Str(s.to_string()), Value::Num),
    }
}

impl Table {
    fn ctx(&self) -> String {
        let place = format!("[{}] at line {}", self.name, self.line);
        if self.file.is_empty() {
            place
        } else {
            format!("{}: {place}", self.file)
        }
    }
    pub fn check_keys(&self, allowed: &[&str]) -> Result<(), String> {
        for k in self.entries.keys() {
            if !allowed.contains(&k.as_str()) {
                return Err(format!(
                    "{}: unknown key '{}' (allowed: {})",
                    self.ctx(),
                    k,
                    allowed.join(", ")
                ));
            }
        }
        Ok(())
    }
    pub fn str_opt(&self, k: &str) -> Result<Option<String>, String> {
        match self.entries.get(k) {
            None => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("{}: '{}' must be a string", self.ctx(), k)),
        }
    }
    pub fn str_req(&self, k: &str) -> Result<String, String> {
        self.str_opt(k)?
            .ok_or_else(|| format!("{}: missing required key '{}'", self.ctx(), k))
    }
    pub fn num_opt(&self, k: &str) -> Result<Option<f64>, String> {
        match self.entries.get(k) {
            None => Ok(None),
            Some(Value::Num(n)) => Ok(Some(*n)),
            Some(_) => Err(format!("{}: '{}' must be a number", self.ctx(), k)),
        }
    }
    pub fn num_req(&self, k: &str) -> Result<f64, String> {
        self.num_opt(k)?
            .ok_or_else(|| format!("{}: missing required key '{}'", self.ctx(), k))
    }
    pub fn bool_opt(&self, k: &str) -> Result<Option<bool>, String> {
        match self.entries.get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(format!("{}: '{}' must be true or false", self.ctx(), k)),
        }
    }
    pub fn str_list(&self, k: &str) -> Result<Vec<String>, String> {
        match self.entries.get(k) {
            Some(Value::Arr(items)) => items
                .iter()
                .map(|v| match v {
                    Value::Str(s) => Ok(s.clone()),
                    _ => Err(format!("{}: '{}' must be a list of strings", self.ctx(), k)),
                })
                .collect(),
            None => Err(format!("{}: missing required key '{}'", self.ctx(), k)),
            _ => Err(format!("{}: '{}' must be a list of strings", self.ctx(), k)),
        }
    }
    pub fn location(&self) -> String {
        self.ctx()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_with_every_kind_of_value() {
        let (sec, e) = parse_line(r#"lane id=main label="Main-Lane \"A\" \\" points=a,b,c x=-2.5 n=1_000 shown=true color=#2563EB"#).unwrap();
        assert_eq!(sec, "lane");
        assert_eq!(e["label"], Value::Str(r#"Main-Lane "A" \"#.into()));
        assert_eq!(e["points"], Value::Arr(vec![Value::Str("a".into()), Value::Str("b".into()), Value::Str("c".into())]));
        assert_eq!(e["x"], Value::Num(-2.5));
        assert_eq!(e["n"], Value::Str("1_000".into()), "only plain numbers");
        assert_eq!(e["shown"], Value::Bool(true));
        assert_eq!(e["color"], Value::Str("#2563EB".into()));
        assert_eq!(e["id"], Value::Str("main".into()));
    }

    #[test]
    fn line_errors() {
        let e = |s: &str| parse_line(s).unwrap_err();
        assert!(e("point id").contains("needs a value"));
        assert!(e("point id=a id=b").contains("duplicate key 'id'"));
        assert!(e("point label=\"open").contains("no closing quote"));
        assert!(e("po-int x=1").contains("section name"));
    }

    #[test]
    fn single_sections_merge_and_lists_keep_their_order() {
        let mut d = Doc::default();
        for line in ["video title=t tail=1", "point id=a", "video tail=2", "point id=b"] {
            let (name, entries) = parse_line(line).unwrap();
            d.add(Table { name, entries, ..Default::default() });
        }
        assert_eq!(d.tables["video"].num_req("tail").unwrap(), 2.0);
        assert_eq!(d.tables["video"].str_req("title").unwrap(), "t");
        let ids: Vec<String> = d.arrays["point"].iter().map(|t| t.str_req("id").unwrap()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn typed_accessors_and_unknown_keys() {
        let (name, entries) = parse_line("video fps=x titel=typo").unwrap();
        let v = Table { name, entries, line: 3, file: "a.mmd".into() };
        assert!(v.num_opt("fps").is_err());
        assert!(v.str_req("missing").is_err());
        assert_eq!(v.num_opt("missing").unwrap(), None);
        let e = v.check_keys(&["title", "fps"]).unwrap_err();
        assert!(e.contains("titel") && e.contains("a.mmd: [video] at line 3"), "{e}");
    }
}
