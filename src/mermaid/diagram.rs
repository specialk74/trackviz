//! The lines of the Mermaid diagram itself: messages and notes.

use super::*;

/// A message of the shared library.
pub struct MsgType {
    pub id: String,
    pub from: String,
    pub to: String,
    pub text: String,
    pub params: String,
    pub reply: bool,
    pub color: Option<String>,
}

impl MsgType {
    pub fn from_table(t: &Table) -> Result<MsgType, String> {
        Ok(MsgType {
            id: t.str_req("id")?,
            from: t.str_req("from")?,
            to: t.str_req("to")?,
            text: t.str_req("text")?,
            params: t.str_opt("params")?.unwrap_or_default(),
            reply: t.bool_opt("reply")?.unwrap_or(false),
            color: t.str_opt("color")?,
        })
    }
}

impl Importer {
    pub(super) fn message(&mut self, line: &str) -> Result<(), String> {
        let (left, label) = line.split_once(':').ok_or("a message needs ': text'")?;
        let arrow = ["-->>", "->>", "--x", "-x", "--)", "-)", "-->", "->"].into_iter().find(|a| left.contains(a)).ok_or("no arrow in the message")?;
        let (a, b) = left.split_once(arrow).unwrap();
        let (a, b) = (a.trim(), b.trim());
        self.participant(a, a);
        self.participant(b, b);
        let (from, to) = (self.actor(a), self.actor(b));
        let reply = arrow.starts_with("--");
        // "Sample Complete SampleId=X Needle=1": the parameters start at the first key=value.
        // `<br/>` starts a new line of the label (drawn so): it also ends a word.
        let mut label = label.to_string();
        for br in ["<br/>", "<br />", "<br>", "<BR/>", "<BR />", "<BR>"] {
            label = label.replace(br, " <br/> ");
        }
        let words: Vec<&str> = label.split_whitespace().collect();
        let split = words.iter().position(|w| w.contains('=')).unwrap_or(words.len());
        let (body, params) = (words[..split].join(" "), &words[split..]);
        if body.is_empty() {
            return Err("a message needs a name".into());
        }
        let value = |key: &str| params.iter().find_map(|p| p.strip_prefix(key)?.strip_prefix('='));
        let sample = value("SampleId");
        let carrier = match (self.next_carrier.take(), sample) {
            (Some(c), _) => Some(c),
            (None, Some(s)) => Some(self.samples.iter().find(|c| c.1 == s).ok_or_else(|| format!("no carrier has sample id '{s}'"))?.0.clone()),
            _ => None,
        };
        let needle = value("Needle");
        // The same message in the library's terms.
        let shape: Vec<String> = params
            .iter()
            .map(|p| match p.split_once('=') {
                Some(("SampleId", _)) => "SampleId={SampleId}".to_string(),
                Some(("Needle", _)) => "Needle={Needle}".to_string(),
                _ => p.to_string(),
            })
            .collect();
        let shape = shape.join(" ");
        let color = self.color.take();
        let types: Vec<MsgType> = self.list("message").iter().map(MsgType::from_table).collect::<Result<_, _>>()?;
        let found = types.iter().find(|m| m.from == from && m.to == to && m.text == body && m.params == shape && m.reply == reply && m.color == color);
        let mut k: Keys = Vec::new();
        match found {
            Some(m) => k.push(("type", text(&m.id))),
            None => {
                k.extend([("from", text(&from)), ("to", text(&to)), ("text", text(&body))]);
                if !shape.is_empty() {
                    k.push(("params", text(&shape)));
                }
                if reply {
                    k.push(("reply", Value::Bool(true)));
                }
                if let Some(c) = &color {
                    k.push(("color", text(c)));
                }
            }
        }
        if let Some(c) = &carrier {
            k.push(("carrier", text(c)));
        }
        let needle_n = match needle {
            Some(n) => Some(n.parse::<f64>().map_err(|_| format!("Needle={n} is not a number"))?),
            None => None,
        };
        if let Some(n) = needle_n {
            k.push(("needle", Value::Num(n)));
        }
        let event = found.map_or_else(|| body.to_lowercase().replace(' ', "_"), |m| m.id.clone());
        if found.is_none() {
            k.push(("event", text(&event)));
        }
        self.step("message", k)
    }

    pub(super) fn note(&mut self, line: &str) -> Result<(), String> {
        let (place, body) = line.split_once(':').ok_or("a note needs ': text'")?;
        let who = place.split_whitespace().last().unwrap_or_default().split(',').next().unwrap_or_default().to_string();
        self.participant(&who, &who);
        // A note right after a pipette is the text of its note on the analyzer.
        if self.last_pipette {
            self.last_pipette = false;
            let step = self.doc.arrays.get_mut("step").and_then(|v| v.last_mut()).ok_or("a note after a pipette needs the pipette")?;
            step.entries.insert("note".to_string(), text(body.trim()));
            return Ok(());
        }
        let actor = self.actor(&who);
        self.step("note", vec![("actor", text(&actor)), ("text", text(body.trim()))])
    }
}
