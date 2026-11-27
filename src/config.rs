use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::channels;

/// Raw format of config.toml: only the contact book, nothing else.
/// Credentials (SMTP, Telegram bot token, ...) are deliberately NOT in
/// here, but come from ENV - separating "who is allowed to be contacted"
/// from "what is used to send".
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(rename = "to", default)]
    to: Vec<ToEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct ToEntry {
    name: String,
    /// All other fields (e.g. "email", "telegram_chat_id") end up here.
    /// Which keys even exist is determined exclusively by the registered
    /// channel plugins (see channels::registry()) - this file doesn't know
    /// a single channel name.
    #[serde(flatten)]
    channels: HashMap<String, String>,
}

/// A contact can have several channels at once (e.g. email AND Telegram) -
/// each combination becomes its own entry here. There is exactly one MCP
/// tool per channel (not per contact): the contact is addressed via the
/// `contact_slug` as a parameter value, validated against this list at
/// call time - never a raw address/chat ID/webhook URL.
#[derive(Debug, Clone)]
pub struct ContactEntry {
    pub contact_name: String,
    /// Stable identifier for this contact, used as the `contact` parameter
    /// value in `send_via_<channel>` and returned by `list_<channel>_contacts`.
    pub contact_slug: String,
    /// Key into the sender map that main.rs builds, e.g. "telegram".
    pub channel_slug: &'static str,
    pub channel_display_name: &'static str,
    /// Email address, Telegram chat ID, etc., depending on the channel.
    pub address: String,
}

/// Groups entries by channel - the shape the MCP server needs, since tools
/// exist per channel, not per contact.
pub fn group_by_channel(entries: Vec<ContactEntry>) -> HashMap<&'static str, Vec<ContactEntry>> {
    let mut grouped: HashMap<&'static str, Vec<ContactEntry>> = HashMap::new();
    for entry in entries {
        grouped.entry(entry.channel_slug).or_default().push(entry);
    }
    grouped
}

pub fn load_contacts(path: &Path) -> Result<Vec<ContactEntry>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("could not read config: {}", path.display()))?;
    let parsed: RawConfig = toml::from_str(&raw)
        .with_context(|| format!("could not parse config: {}", path.display()))?;

    if parsed.to.is_empty() {
        bail!("config.toml does not contain a single [[to]] entry — there would be no tool to send with");
    }

    let defs = channels::registry();
    let mut seen_slugs: HashMap<String, String> = HashMap::new();
    let mut tools = Vec::new();

    for entry in parsed.to {
        let name = entry.name.trim().to_string();
        if name.is_empty() {
            bail!("a [[to]] entry has an empty name");
        }

        let slug = slugify(&name);
        if let Some(existing_name) = seen_slugs.insert(slug.clone(), name.clone()) {
            bail!(
                "name collision in config.toml: '{existing_name}' and '{name}' produce the \
                 same slug '{slug}' and therefore ambiguous tool names. Please make one of the \
                 two names unique (e.g. add a last name) — the server deliberately refuses to \
                 start with ambiguous tool names."
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
                "contact '{name}' has unknown fields: {} (known channel fields: {})",
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
                .with_context(|| format!("contact '{name}' has an invalid value for '{}'", def.config_key))?;

            tools.push(ContactEntry {
                contact_name: name.clone(),
                contact_slug: slug.clone(),
                channel_slug: def.slug,
                channel_display_name: def.display_name,
                address,
            });
        }

        if !found_any_channel {
            bail!(
                "contact '{name}' has not a single channel configured (known channel fields: {})",
                defs.iter().map(|def| def.config_key).collect::<Vec<_>>().join(", ")
            );
        }
    }

    Ok(tools)
}

/// Produces a stable, tool-friendly slug from a display name. German
/// umlauts are transliterated, everything else is reduced to [a-z0-9_].
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
    let mut last_was_sep = true; // prevents a leading underscore
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

    /// Umlaut transliteration is a real feature (German contact names are
    /// an expected, supported input), not project-language leftovers - this
    /// test case intentionally keeps German names.
    #[test]
    fn slugify_umlauts() {
        assert_eq!(slugify("Jürgen Müller"), "juergen_mueller");
        assert_eq!(slugify("Max Mustermann"), "max_mustermann");
        assert_eq!(slugify("  Anna-Lena  "), "anna_lena");
    }

    /// Returns the `TempDir` guard instead of dropping it - otherwise
    /// `tempfile` would already delete the directory (via RAII, see its
    /// `Drop` impl) before the actual `load_contacts()` call. The caller
    /// binds it as `let (_dir, path) = write_config(...)` so it stays alive
    /// until the end of the test case.
    fn write_config(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn detects_slug_collision() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "Juergen Schmidt"
email = "j1@example.com"

[[to]]
name = "Juergen  Schmidt"
email = "j2@example.com"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("name collision"));
    }

    #[test]
    fn contact_needs_at_least_one_channel() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "No Channel"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("not a single channel"));
    }

    #[test]
    fn rejects_unknown_field() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "Max Mustermann"
emial = "max@example.com"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("unknown fields"));
    }

    #[test]
    fn rejects_invalid_email() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "Max Mustermann"
email = "not-valid"
"#,
        );
        let err = load_contacts(&path).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn generates_contact_slug_and_channel_metadata() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "Max Mustermann"
email = "max@example.com"
"#,
        );
        let tools = load_contacts(&path).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].contact_slug, "max_mustermann");
        assert_eq!(tools[0].channel_slug, "email");
        assert_eq!(tools[0].channel_display_name, "Email");
    }

    #[test]
    fn group_by_channel_groups_multiple_contacts_per_channel() {
        let (_dir, path) = write_config(
            r#"
[[to]]
name = "Max Mustermann"
email = "max@example.com"

[[to]]
name = "Erika Musterfrau"
email = "erika@example.com"
"#,
        );
        let tools = load_contacts(&path).unwrap();
        let grouped = group_by_channel(tools);
        assert_eq!(grouped.get("email").map(Vec::len), Some(2));
    }
}
