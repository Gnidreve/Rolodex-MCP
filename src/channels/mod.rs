use anyhow::Result;
use async_trait::async_trait;

mod email;

/// Ein Kanal-Plugin kapselt alles Kanal-Spezifische: wie ENV-Konfiguration
/// geladen/geprüft wird und wie tatsächlich verschickt wird. `config.rs`,
/// `mcp_server.rs` und `main.rs` kennen nur diesen Trait (und `ChannelDef`
/// unten), nie ein konkretes E-Mail/Telegram/... - ein neuer Kanal kommt als
/// eigene Datei dazu, plus genau einer neuen Zeile in `registry()` unten.
///
/// Projektregel (siehe CONTRIBUTING.md): jeder Kanal wird auf einer eigenen
/// Git-Branch entwickelt und per Pull Request nach `main` gemerged, ohne
/// main.rs/config.rs/mcp_server.rs anzufassen.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Baut die Verbindung/den Zugang einmal real auf, ohne zu senden - für
    /// den Startup-Check. Ein Fehler hier lässt den Server gar nicht erst
    /// starten, statt erst beim ersten echten Sendeversuch aufzufallen.
    async fn test_connection(&self) -> Result<()>;

    /// Verschickt eine Nachricht. `contact_name` ist der Anzeigename aus
    /// config.toml (manche Kanäle nutzen ihn im Empfänger, andere ignorieren
    /// ihn - das entscheidet jeder Kanal selbst).
    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()>;
}

/// Statische Metadaten eines Kanals, unabhängig davon, ob er gerade aktiv
/// konfiguriert ist (das entscheidet erst `config.toml` zur Laufzeit).
pub struct ChannelDef {
    /// Feldname in config.toml, z.B. "email" oder "telegram_chat_id".
    pub config_key: &'static str,
    /// Segment im Tool-Namen, z.B. "email" -> "send_to_<name>_via_email".
    pub slug: &'static str,
    /// Für den menschenlesbaren Tool-Titel/Beschreibung, z.B. "E-Mail".
    pub display_name: &'static str,
    /// Prüft den Adress-/ID-Wert aus config.toml auf Plausibilität (reines
    /// Format, keine Netzwerkchecks - die macht `Channel::test_connection`).
    pub validate: fn(&str) -> Result<()>,
    /// Lädt die ENV-Konfiguration für diesen Kanal. Wird von main.rs nur
    /// aufgerufen, wenn mindestens ein Kontakt `config_key` tatsächlich nutzt.
    pub from_env: fn() -> Result<Box<dyn Channel>>,
}

/// Alle bekannten Kanäle. Neuer Kanal = eigene Datei (auf eigener Branch,
/// eigenem PR) + genau eine neue Zeile hier.
pub fn registry() -> Vec<ChannelDef> {
    vec![email::channel_def()]
}
