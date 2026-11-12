//! Antennas, the devices `pipette` and `needle` work on, known carriers.

use super::*;

impl Importer {
    /// The antennas of the scene, also those declared on a device: (id, point).
    pub(super) fn antennas(&self) -> Vec<(String, String)> {
        let field = |t: &Table, k: &str| t.str_opt(k).ok().flatten().unwrap_or_default();
        self.list("device")
            .iter()
            .filter_map(|t| match (field(t, "kind").as_str(), field(t, "antenna")) {
                ("antenna", _) => Some((field(t, "id"), field(t, "at"))),
                // The antenna of a turnstile is under the needle.
                ("turnstile", a) if !a.is_empty() => Some((a, format!("{}.needle", field(t, "id")))),
                // An antenna declared on another device: antenna=antenna2.
                (_, a) if !a.is_empty() => Some((a, field(t, "at"))),
                _ => None,
            })
            .collect()
    }

    /// The antenna under the needle of a turnstile: the wheel brings carriers
    /// there, so a step can't wait for one to get on it.
    pub(super) fn not_in_a_wheel(cmd: &str, antenna: &str, point: &str) -> Result<(), String> {
        match point.strip_suffix(".needle") {
            Some(ts) => Err(format!("'{cmd} {antenna}': the wheel brings the carrier under the needle; start from the rotation: after <seconds> rotate {ts}")),
            None => Ok(()),
        }
    }

    /// The point an antenna reads.
    pub(super) fn antenna_point(&self, antenna: &str) -> Result<String, String> {
        let antennas = self.antennas();
        antennas.iter().find(|a| a.0 == antenna).map(|a| a.1.clone()).ok_or_else(|| {
            let mut names: Vec<&str> = antennas.iter().map(|a| a.0.as_str()).collect();
            names.sort();
            format!("'{antenna}' is not an antenna of this scene (antennas: {})", if names.is_empty() { "none".to_string() } else { names.join(", ") })
        })
    }

    /// Where `pipette` and `needle` work: a sampling point by its antenna (the
    /// device's own name is refused), a turnstile by its name (no antenna there).
    pub(super) fn probe(&self, cmd: &str, name: &str) -> Result<String, String> {
        let field = |t: &Table, k: &str| t.str_opt(k).ok().flatten().unwrap_or_default();
        let devices = self.list("device");
        if let Some(t) = devices.iter().find(|t| field(t, "antenna") == name) {
            return match field(t, "kind").as_str() {
                "sampling" | "turnstile" => Ok(field(t, "id")),
                _ => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            };
        }
        match devices.iter().find(|t| field(t, "id") == name) {
            Some(t) if ["sampling", "turnstile"].contains(&field(t, "kind").as_str()) && !field(t, "antenna").is_empty() => {
                Err(format!("'{cmd} {name}': write the antenna, {cmd} {}", field(t, "antenna")))
            }
            // A turnstile, or a sampling point without an antenna: by its name.
            Some(t) if ["turnstile", "sampling"].contains(&field(t, "kind").as_str()) => Ok(name.to_string()),
            Some(_) => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            None if self.antennas().iter().any(|a| a.0 == name) => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            None => Err(format!("'{cmd} {name}': not an antenna of this scene")),
        }
    }

    pub(super) fn known_carrier(&self, what: &str, c: &str) -> Result<(), String> {
        if self.samples.iter().any(|s| s.0 == c) {
            Ok(())
        } else {
            Err(format!("'{what}': no carrier '{c}'"))
        }
    }
}
