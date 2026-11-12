//! When the next step starts: `on`, `after`, `at`, and `id` to name a step
//! that a later `after` waits for.

use super::*;

impl Importer {
    /// A step has one starting rule: only `delay` is added to another.
    pub(super) fn no_start_yet(&self) -> Result<(), String> {
        if self.on.is_some() || self.timing.iter().any(|(k, _)| ["when", "at", "after"].contains(k)) {
            return Err("only one of on, after, at for a step".into());
        }
        Ok(())
    }

    pub(super) fn timing_key(&mut self, key: &'static str, value: Value) -> Result<(), String> {
        if key != "delay" {
            self.no_start_yet()?;
        }
        self.timing.push((key, value));
        Ok(())
    }

    /// `%% on <antenna> [<carrier>]`: the next step starts when the carrier is over
    /// the antenna; without a carrier, the one of the step (its SampleId, or the
    /// carrier it moves or shows).
    pub(super) fn on_antenna(&mut self, antenna: &str, carrier: Option<&str>) -> Result<(), String> {
        let point = self.antenna_point(antenna)?;
        Self::not_in_a_wheel("on", antenna, &point)?;
        if let Some(c) = carrier {
            self.known_carrier(&format!("on {antenna} {c}"), c)?;
        }
        self.no_start_yet()?;
        self.on = Some((point, antenna.to_string(), carrier.map(String::from)));
        Ok(())
    }

    /// `%% after <seconds>`: after the end of the step before. With what it waits
    /// for: `<id>` when the step named so with `%% id` ends; `<antenna> <carrier>`
    /// when the carrier is over the antenna; `pipette|rotate|needle <device>` when
    /// that action ends.
    pub(super) fn after(&mut self, secs: Value, refer: &[&str]) -> Result<(), String> {
        if refer.is_empty() {
            return self.timing_key("after", secs);
        }
        if self.antennas().iter().any(|a| a.0 == refer[0]) {
            let [antenna, c] = refer else {
                return Err(format!("'after <seconds> {}' needs the carrier: after <seconds> {} <carrier>", refer[0], refer[0]));
            };
            let point = self.antenna_point(antenna)?;
            Self::not_in_a_wheel("after <seconds>", antenna, &point)?;
            self.known_carrier(&format!("after {antenna} {c}"), c)?;
            self.timing_key("when", text(&format!("{c} at {point}")))?;
            return self.timing_key("delay", secs);
        }
        if let [kind @ ("pipette" | "rotate" | "needle"), device] = refer {
            let device = if *kind == "rotate" { device.to_string() } else { self.probe(kind, device)? };
            self.timing_key("when", text(&format!("{kind} {device} done")))?;
            return self.timing_key("delay", secs);
        }
        let [id] = refer else {
            return Err(format!(
                "'after <seconds> {}': write the id of a line (after <seconds> <id>, with %% id <id> above that line), an antenna and a carrier, or pipette|rotate|needle <device>",
                refer.join(" ")
            ));
        };
        if !self.ids.iter().any(|i| i == id) {
            return Err(format!("'after': no line with id '{id}' before this one (write %% id {id} above the line to wait for)"));
        }
        self.timing_key("when", text(&format!("step {id} done")))?;
        self.timing_key("delay", secs)
    }

    /// `%% id <name>`: names the next step, so that a later `%% after` can wait for it.
    pub(super) fn name_next(&mut self, id: &str) -> Result<(), String> {
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("an id has only letters, digits, '-' and '_': '{id}'"));
        }
        if ["pipette", "rotate", "needle"].contains(&id) || self.antennas().iter().any(|a| a.0 == id) {
            return Err(format!("'{id}' is already the name of an antenna or an action: choose another id"));
        }
        if self.ids.iter().any(|i| i == id) {
            return Err(format!("the id '{id}' is already used: each line has its own"));
        }
        if let Some(prev) = &self.next_id {
            return Err(format!("'id {id}' after 'id {prev}': a line has one id"));
        }
        self.ids.push(id.to_string());
        self.next_id = Some(id.to_string());
        Ok(())
    }
}
