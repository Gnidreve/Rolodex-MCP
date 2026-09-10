use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Rohformat der config.toml: nur das Kontaktbuch, sonst nichts.
/// SMTP-Zugangsdaten kommen bewusst NICHT hier rein, sondern aus ENV
/// (siehe smtp.rs) — Trennung von "wer darf angeschrieben werden"
/// und "womit wird verschickt".
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(rename = "to", default)]
    to: Vec<ToEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct ToEntry {
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
pub struct Contact {
    pub name: String,
    pub email: String,
    /// z.B. "max_mustermann"
    pub slug: String,
    /// z.B. "send_email_to_max_mustermann" — der MCP-Tool-Name
    pub tool_name: String,
}

pub fn load_contacts(path: &Path) -> Result<Vec<Contact>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("konnte config nicht lesen: {}", path.display()))?;
    let parsed: RawConfig = toml::from_str(&raw)
        .with_context(|| format!("konnte config nicht parsen: {}", path.display()))?;

    if parsed.to.is_empty() {
        bail!("config.toml enthält keinen einzigen [[to]]-Eintrag — es gäbe kein einziges Tool zum Versenden");
    }

    let mut seen: HashMap<String, String> = HashMap::new();
    let mut contacts = Vec::with_capacity(parsed.to.len());

    for entry in parsed.to {
        let name = entry.name.trim().to_string();
        let email = entry.email.trim().to_string();

        if name.is_empty() {
            bail!("ein [[to]]-Eintrag hat einen leeren name");
        }
        if email.is_empty() || !email.contains('@') {
            bail!("Kontakt '{name}' hat keine gültige email ('{email}')");
        }

        let slug = slugify(&name);
        let tool_name = format!("send_email_to_{slug}");

        if let Some(existing_name) = seen.insert(slug.clone(), name.clone()) {
            bail!(
                "Namenskollision in config.toml: '{existing_name}' und '{name}' erzeugen \
                 beide das Tool '{tool_name}'. Bitte einen der beiden Namen eindeutig machen \
                 (z.B. Nachname ergänzen) — der Server startet bewusst nicht mit \
                 mehrdeutigen Tool-Namen."
            );
        }

        contacts.push(Contact {
            name,
            email,
            slug,
            tool_name,
        });
    }

    Ok(contacts)
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

    #[test]
    fn detects_slug_collision() {
        let dir = std::env::temp_dir().join(format!("sendmail-mcp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[[to]]
name = "Jürgen Schmidt"
email = "j1@example.com"

[[to]]
name = "Jürgen  Schmidt"
email = "j2@example.com"
"#,
        )
        .unwrap();
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("Namenskollision"));
    }
}
