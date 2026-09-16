use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::channels;

/// Rohformat der config.toml: nur das Kontaktbuch, sonst nichts.
/// Zugangsdaten (SMTP, Telegram-Bot-Token, ...) kommen bewusst NICHT hier
/// rein, sondern aus ENV — Trennung von "wer darf angeschrieben werden" und
/// "womit wird verschickt".
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(rename = "to", default)]
    to: Vec<ToEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct ToEntry {
    name: String,
    /// Alle anderen Felder (z.B. "email", "telegram_chat_id") landen hier.
    /// Welche Keys es überhaupt gibt, bestimmen ausschließlich die
    /// registrierten Channel-Plugins (siehe channels::registry()) - diese
    /// Datei kennt keinen einzigen Kanalnamen.
    #[serde(flatten)]
    channels: HashMap<String, String>,
}

/// Ein Kontakt kann mehrere Kanäle gleichzeitig haben (z.B. E-Mail UND
/// Telegram). Pro Kontakt UND Kanal entsteht ein eigenes, konkretes Tool -
/// kein generisches "sende Nachricht", weil ein Kontakt sonst mehrdeutig
/// über mehr als einen Kanal ansprechbar wäre, ohne dass der Agent das im
/// Tool-Namen sehen könnte.
#[derive(Debug, Clone)]
pub struct ChannelTool {
    pub contact_name: String,
    /// Schlüssel in die Sender-Map, die main.rs baut, z.B. "telegram".
    pub channel_slug: &'static str,
    pub channel_display_name: &'static str,
    /// E-Mail-Adresse, Telegram-Chat-ID, o.ä., je nach Kanal.
    pub address: String,
    /// z.B. "send_to_max_mustermann_via_telegram" — der MCP-Tool-Name.
    /// Name zuerst, Kanal als Suffix: so bleiben die Tools eines Kontakts
    /// auch in alphabetisch sortierenden Clients nebeneinander, statt nach
    /// Kanal in getrennte Blöcke zu zerfallen.
    pub tool_name: String,
    /// z.B. "Max Mustermann — Telegram" - für Clients, die MCPs `title`-Feld
    /// zusätzlich zum `name` anzeigen.
    pub title: String,
}

pub fn load_contacts(path: &Path) -> Result<Vec<ChannelTool>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("konnte config nicht lesen: {}", path.display()))?;
    let parsed: RawConfig = toml::from_str(&raw)
        .with_context(|| format!("konnte config nicht parsen: {}", path.display()))?;

    if parsed.to.is_empty() {
        bail!("config.toml enthält keinen einzigen [[to]]-Eintrag — es gäbe kein einziges Tool zum Versenden");
    }

    let defs = channels::registry();
    let mut seen_slugs: HashMap<String, String> = HashMap::new();
    let mut tools = Vec::new();

    for entry in parsed.to {
        let name = entry.name.trim().to_string();
        if name.is_empty() {
            bail!("ein [[to]]-Eintrag hat einen leeren name");
        }

        let slug = slugify(&name);
        if let Some(existing_name) = seen_slugs.insert(slug.clone(), name.clone()) {
            bail!(
                "Namenskollision in config.toml: '{existing_name}' und '{name}' ergeben \
                 denselben Slug '{slug}' und damit mehrdeutige Tool-Namen. Bitte einen der \
                 beiden Namen eindeutig machen (z.B. Nachname ergänzen) — der Server startet \
                 bewusst nicht mit mehrdeutigen Tool-Namen."
            );
        }

        let mut unrecognized: Vec<&str> = entry
            .channels
            .keys()
            .map(String::as_str)
            .filter(|key| !defs.iter().any(|def| def.config_key == *key))
            .collect();
        if !unrecognized.is_empty() {
            unrecognized.sort_unstable();
            bail!(
                "Kontakt '{name}' hat unbekannte Felder: {} (bekannte Kanal-Felder: {})",
                unrecognized.join(", "),
                defs.iter().map(|def| def.config_key).collect::<Vec<_>>().join(", ")
            );
        }

        let mut found_any_channel = false;
        for def in &defs {
            let Some(raw_value) = entry.channels.get(def.config_key) else {
                continue;
            };
            let address = raw_value.trim().to_string();
            if address.is_empty() {
                continue;
            }
            found_any_channel = true;

            (def.validate)(&address)
                .with_context(|| format!("Kontakt '{name}' hat einen ungültigen Wert für '{}'", def.config_key))?;

            tools.push(ChannelTool {
                contact_name: name.clone(),
                channel_slug: def.slug,
                channel_display_name: def.display_name,
                address,
                tool_name: format!("send_to_{slug}_via_{}", def.slug),
                title: format!("{name} — {}", def.display_name),
            });
        }

        if !found_any_channel {
            bail!(
                "Kontakt '{name}' hat keinen einzigen Kanal konfiguriert (bekannte Kanal-Felder: {})",
                defs.iter().map(|def| def.config_key).collect::<Vec<_>>().join(", ")
            );
        }
    }

    Ok(tools)
}

/// Erzeugt aus einem Anzeigenamen einen stabilen, tool-tauglichen Slug.
/// Deutsche Umlaute werden transliteriert, alles andere auf [a-z0-9_] reduziert.
fn slugify(name: &str) -> String {
    let normalized: String = name
        .chars()
        .map(|c| match c {
            'ä' => "ae".to_string(),
            'ö' => "oe".to_string(),
            'ü' => "ue".to_string(),
            'Ä' => "Ae".to_string(),
            'Ö' => "Oe".to_string(),
            'Ü' => "Ue".to_string(),
            'ß' => "ss".to_string(),
            other => other.to_string(),
        })
        .collect();

    let mut out = String::new();
    let mut last_was_sep = true; // verhindert führenden Unterstrich
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("contact");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_umlauts() {
        assert_eq!(slugify("Jürgen Müller"), "juergen_mueller");
        assert_eq!(slugify("Max Mustermann"), "max_mustermann");
        assert_eq!(slugify("  Anna-Lena  "), "anna_lena");
    }

    fn write_config(contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sendmail-mcp-test-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[test]
    fn detects_slug_collision() {
        let path = write_config(
            r#"
[[to]]
name = "Jürgen Schmidt"
email = "j1@example.com"

[[to]]
name = "Jürgen  Schmidt"
email = "j2@example.com"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("Namenskollision"));
    }

    #[test]
    fn contact_needs_at_least_one_channel() {
        let path = write_config(
            r#"
[[to]]
name = "Ohne Kanal"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("keinen einzigen Kanal"));
    }

    #[test]
    fn rejects_unknown_field() {
        let path = write_config(
            r#"
[[to]]
name = "Max Mustermann"
emial = "max@example.com"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("unbekannte Felder"));
    }

    #[test]
    fn rejects_invalid_email() {
        let path = write_config(
            r#"
[[to]]
name = "Max Mustermann"
email = "nicht-valide"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("ungültigen Wert"));
    }

    #[test]
    fn generates_name_first_tool_name_and_title() {
        let path = write_config(
            r#"
[[to]]
name = "Max Mustermann"
email = "max@example.com"
"#,
        );
        let tools = load_contacts(&path).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_name, "send_to_max_mustermann_via_email");
        assert_eq!(tools[0].title, "Max Mustermann — E-Mail");
    }
}
