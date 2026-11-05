//! Minimal TOML-subset parser (no external crates).
//! Supports: comments, [table], [[array_of_tables]], key = value,
//! strings ("..." with \" \\ \n \t escapes), numbers, booleans,
//! and arrays (possibly spanning multiple lines).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
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
    /// The included file this table comes from (empty: the scene file itself).
    pub file: String,
    pub entries: BTreeMap<String, Value>,
}

#[derive(Debug, Default)]
pub struct Doc {
    pub tables: BTreeMap<String, Table>,
    pub arrays: BTreeMap<String, Vec<Table>>,
}

enum Cur {
    Table(String),
    Arr(String),
}

/// Reads a scene file with its includes. A top-level `include = ["a.toml", ...]`
/// loads other files first (paths relative to the including file, nested includes
/// allowed): their array sections come before the ones of this file, and the keys
/// of a table such as [video] are merged, the including file winning.
pub fn parse_file(path: &Path) -> Result<Doc, String> {
    parse_included(path, "", &mut Vec::new())
}

fn parse_included(path: &Path, shown: &str, stack: &mut Vec<PathBuf>) -> Result<Doc, String> {
    let name = path.display();
    let real = path.canonicalize().map_err(|e| format!("{name}: {e}"))?;
    if stack.contains(&real) {
        return Err(format!("{name}: circular include"));
    }
    let src = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
    let mut own = parse(&src).map_err(|e| if shown.is_empty() { e } else { format!("{shown}: {e}") })?;
    for t in own.tables.values_mut().chain(own.arrays.values_mut().flatten()) {
        t.file = shown.to_string();
    }
    let root = own.tables.get_mut("").unwrap();
    let includes = match root.entries.contains_key("include") {
        true => root.str_list("include")?,
        false => Vec::new(),
    };
    root.entries.remove("include");

    stack.push(real);
    let mut doc = Doc::default();
    for inc in &includes {
        let target = path.parent().unwrap_or(Path::new("")).join(inc);
        let d = parse_included(&target, inc, stack)?;
        merge(&mut doc, d);
    }
    stack.pop();
    merge(&mut doc, own);
    Ok(doc)
}

/// Adds `other` after `doc`: arrays are appended, table keys override.
fn merge(doc: &mut Doc, other: Doc) {
    for (k, t) in other.tables {
        match doc.tables.get_mut(&k) {
            Some(d) => d.entries.extend(t.entries),
            None => {
                doc.tables.insert(k, t);
            }
        }
    }
    for (k, v) in other.arrays {
        doc.arrays.entry(k).or_default().extend(v);
    }
}

pub fn parse(src: &str) -> Result<Doc, String> {
    let mut doc = Doc::default();
    doc.tables.insert(
        String::new(),
        Table { name: "(root)".into(), line: 0, ..Default::default() },
    );
    let mut cur = Cur::Table(String::new());
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let lineno = i + 1;
        let line = strip_comment(lines[i]).trim().to_string();
        i += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") {
            if !line.ends_with("]]") {
                return Err(format!("line {lineno}: malformed [[array]] header"));
            }
            let name = line[2..line.len() - 2].trim().to_string();
            doc.arrays.entry(name.clone()).or_default().push(Table {
                name: name.clone(),
                line: lineno,
                ..Default::default()
            });
            cur = Cur::Arr(name);
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("line {lineno}: malformed [table] header"));
            }
            let name = line[1..line.len() - 1].trim().to_string();
            if doc.tables.contains_key(&name) {
                return Err(format!("line {lineno}: table [{name}] defined twice"));
            }
            doc.tables.insert(
                name.clone(),
                Table { name: name.clone(), line: lineno, ..Default::default() },
            );
            cur = Cur::Table(name);
            continue;
        }
        let eq = find_eq(&line).ok_or_else(|| format!("line {lineno}: expected key = value"))?;
        let key = line[..eq].trim().trim_matches('"').to_string();
        if key.is_empty() {
            return Err(format!("line {lineno}: empty key"));
        }
        let mut val = line[eq + 1..].trim().to_string();
        while bracket_depth(&val) > 0 {
            if i >= lines.len() {
                return Err(format!("line {lineno}: unterminated array"));
            }
            val.push(' ');
            val.push_str(strip_comment(lines[i]).trim());
            i += 1;
        }
        let (v, rest) = parse_value(&val).map_err(|e| format!("line {lineno}: {e}"))?;
        if !rest.trim().is_empty() {
            return Err(format!("line {lineno}: unexpected trailing text '{}'", rest.trim()));
        }
        let table = match &cur {
            Cur::Table(n) => doc.tables.get_mut(n).unwrap(),
            Cur::Arr(n) => doc.arrays.get_mut(n).unwrap().last_mut().unwrap(),
        };
        if table.entries.insert(key.clone(), v).is_some() {
            return Err(format!("line {lineno}: duplicate key '{key}'"));
        }
    }
    Ok(doc)
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let mut prev = '\0';
    for (idx, c) in line.char_indices() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        } else if c == '#' && !in_str {
            return &line[..idx];
        }
        prev = c;
    }
    line
}

fn find_eq(line: &str) -> Option<usize> {
    let mut in_str = false;
    for (idx, c) in line.char_indices() {
        if c == '"' {
            in_str = !in_str;
        } else if c == '=' && !in_str {
            return Some(idx);
        }
    }
    None
}

fn bracket_depth(s: &str) -> i32 {
    let mut d = 0;
    let mut in_str = false;
    let mut prev = '\0';
    for c in s.chars() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        } else if !in_str {
            if c == '[' {
                d += 1;
            } else if c == ']' {
                d -= 1;
            }
        }
        prev = c;
    }
    d
}

fn parse_value(s: &str) -> Result<(Value, &str), String> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.char_indices();
        while let Some((idx, c)) = chars.next() {
            match c {
                '"' => return Ok((Value::Str(out), &rest[idx + 1..])),
                '\\' => match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, '"')) => out.push('"'),
                    Some((_, '\\')) => out.push('\\'),
                    Some((_, o)) => return Err(format!("unknown escape \\{o}")),
                    None => break,
                },
                _ => out.push(c),
            }
        }
        return Err("unterminated string".into());
    }
    if let Some(mut rest) = s.strip_prefix('[') {
        let mut items = Vec::new();
        loop {
            rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix(']') {
                return Ok((Value::Arr(items), r));
            }
            let (v, r) = parse_value(rest)?;
            items.push(v);
            rest = r.trim_start();
            if let Some(r) = rest.strip_prefix(',') {
                rest = r;
            } else if !rest.starts_with(']') {
                return Err("expected ',' or ']' in array".into());
            }
        }
    }
    if let Some(r) = s.strip_prefix("true") {
        return Ok((Value::Bool(true), r));
    }
    if let Some(r) = s.strip_prefix("false") {
        return Ok((Value::Bool(false), r));
    }
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || "+-._eE".contains(c)))
        .unwrap_or(s.len());
    if end == 0 {
        return Err(format!("cannot parse value '{s}' (strings need double quotes)"));
    }
    let num: f64 = s[..end]
        .replace('_', "")
        .parse()
        .map_err(|_| format!("invalid number '{}'", &s[..end]))?;
    Ok((Value::Num(num), &s[end..]))
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
    fn parses_tables_arrays_and_values() {
        let doc = parse(
            r#"
# comment
[video]
title = "a # not a comment \"q\" \\ x" # trailing comment
fps = 1_000
tail = -2.5
show = true

[[point]]
id = "p1"
list = ["a",
        "b"]

[[point]]
id = "p2"
"#,
        )
        .unwrap();
        let v = &doc.tables["video"];
        assert_eq!(v.str_req("title").unwrap(), r#"a # not a comment "q" \ x"#);
        assert_eq!(v.num_req("fps").unwrap(), 1000.0);
        assert_eq!(v.num_req("tail").unwrap(), -2.5);
        assert_eq!(v.bool_opt("show").unwrap(), Some(true));
        let pts = &doc.arrays["point"];
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].str_list("list").unwrap(), vec!["a", "b"]);
        assert_eq!(pts[1].str_req("id").unwrap(), "p2");
    }

    /// Writes files in a fresh temporary directory, returning it.
    fn files(tag: &str, list: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trackviz_inc_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, text) in list {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    #[test]
    fn includes_are_merged_and_the_scene_wins() {
        let dir = files(
            "merge",
            &[
                ("s.toml", "include = [\"common/a.toml\", \"common/b.toml\"]\n[video]\ntail = 9\n[[point]]\nid = \"s\"\n"),
                ("common/a.toml", "include = [\"c.toml\"]\n[video]\ntitle = \"t\"\ntail = 1\n[[point]]\nid = \"a\"\n"),
                ("common/b.toml", "[[point]]\nid = \"b\"\n"),
                ("common/c.toml", "[[point]]\nid = \"c\"\n"),
            ],
        );
        let doc = parse_file(&dir.join("s.toml")).unwrap();
        let v = &doc.tables["video"];
        assert_eq!(v.str_req("title").unwrap(), "t");
        assert_eq!(v.num_req("tail").unwrap(), 9.0);
        let ids: Vec<String> = doc.arrays["point"].iter().map(|t| t.str_req("id").unwrap()).collect();
        // Nested first, then in include order, then the scene's own.
        assert_eq!(ids, vec!["c", "a", "b", "s"]);
        assert!(doc.tables[""].entries.is_empty(), "include is consumed");
        // Errors in an included table name its file.
        assert!(doc.arrays["point"][1].location().contains("common/a.toml"), "{}", doc.arrays["point"][1].location());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn include_errors() {
        let dir = files(
            "err",
            &[
                ("loop1.toml", "include = [\"loop2.toml\"]\n"),
                ("loop2.toml", "include = [\"loop1.toml\"]\n"),
                ("missing.toml", "include = [\"nope.toml\"]\n"),
                ("bad.toml", "include = [\"broken.toml\"]\n"),
                ("broken.toml", "[video\n"),
            ],
        );
        assert!(parse_file(&dir.join("loop1.toml")).unwrap_err().contains("circular include"));
        assert!(parse_file(&dir.join("missing.toml")).unwrap_err().contains("nope.toml"));
        assert!(parse_file(&dir.join("bad.toml")).unwrap_err().contains("broken.toml"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_key_reports_the_line() {
        let e = parse("[video]\nfps = 1\nfps = 2\n").unwrap_err();
        assert!(e.contains("line 3"), "{e}");
    }

    #[test]
    fn typed_accessors_and_unknown_keys() {
        let doc = parse("[video]\nfps = \"x\"\ntitel = \"typo\"\n").unwrap();
        let v = &doc.tables["video"];
        assert!(v.num_opt("fps").is_err());
        assert!(v.str_req("missing").is_err());
        assert_eq!(v.num_opt("missing").unwrap(), None);
        let e = v.check_keys(&["title", "fps"]).unwrap_err();
        assert!(e.contains("titel"), "{e}");
    }
}
