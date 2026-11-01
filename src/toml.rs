//! Minimal TOML-subset parser (no external crates).
//! Supports: comments, [table], [[array_of_tables]], key = value,
//! strings ("..." with \" \\ \n \t escapes), numbers, booleans,
//! and arrays (possibly spanning multiple lines).

use std::collections::BTreeMap;

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
        format!("[{}] at line {}", self.name, self.line)
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
