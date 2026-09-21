use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::{Channel, ChannelDef};

pub fn channel_def() -> ChannelDef {
    ChannelDef {
        config_key: "ntfy_topic",
        slug: "ntfy",
        display_name: "ntfy",
        validate: validate_topic,
        from_env,
    }
}

/// ntfy-Topics sind faktisch das Secret (wer den Topic-Namen kennt, kann
/// mitlesen) - der erlaubte Zeichensatz ist bewusst konservativ, damit kein
/// Wert versehentlich die URL-Pfadstruktur ("/") oder HTTP-Header (Whitespace)
/// durcheinanderbringt.
fn validate_topic(topic: &str) -> Result<()> {
    let valid = !topic.is_empty()
        && topic
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        bail!("kein gültiges ntfy-Topic ('{topic}') - erlaubt sind nur Buchstaben, Zahlen, '_' und '-'");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(NtfyConfig::from_env()?))
}

/// Server + optionaler Access-Token kommen aus ENV, nie aus config.toml.
#[derive(Debug, Clone)]
struct NtfyConfig {
    /// Ohne abschließenden "/", z.B. "https://ntfy.sh" (Default) oder eine
    /// selbst gehostete Instanz.
    server: String,
    /// Nur nötig, wenn der Server/das Topic Zugriffskontrolle verlangt -
    /// der öffentliche ntfy.sh-Dienst braucht i.d.R. keinen.
    access_token: Option<String>,
}

impl NtfyConfig {
    fn from_env() -> Result<Self> {
        let server = non_empty(std::env::var("NTFY_SERVER").ok())
            .unwrap_or_else(|| "https://ntfy.sh".to_string())
            .trim_end_matches('/')
            .to_string();
        let access_token = non_empty(std::env::var("NTFY_ACCESS_TOKEN").ok());
        Ok(Self { server, access_token })
    }

    fn client(&self) -> reqwest::Client {
        reqwest::Client::new()
    }

    fn authorize(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.access_token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    healthy: bool,
}

#[async_trait]
impl Channel for NtfyConfig {
    /// ntfy hat keine Auth-Prüfung pro Topic (Topics werden implizit beim
    /// ersten Publish angelegt) - `test_connection` prüft stattdessen nur,
    /// ob der Server erreichbar und gesund ist (`GET /v1/health`).
    async fn test_connection(&self) -> Result<()> {
        let url = format!("{}/v1/health", self.server);
        let response = self
            .authorize(self.client().get(&url))
            .send()
            .await
            .context("ntfy-Verbindungstest fehlgeschlagen (Netzwerk)")?;
        let status = response.status();
        if !status.is_success() {
            bail!("ntfy-Verbindungstest fehlgeschlagen: HTTP {status}");
        }
        let health: HealthResponse = response.json().await.context("ntfy-Health-Antwort nicht lesbar")?;
        if !health.healthy {
            bail!("ntfy-Verbindungstest fehlgeschlagen: Server meldet sich nicht als 'healthy'");
        }
        Ok(())
    }

    async fn send(&self, _contact_name: &str, topic: &str, subject: &str, body: &str) -> Result<()> {
        let url = format!("{}/{topic}", self.server);
        let response = self
            .authorize(self.client().post(&url))
            .header("Title", subject)
            .body(body.to_string())
            .send()
            .await
            .context("ntfy-Versand fehlgeschlagen (Netzwerk)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("ntfy-Versand fehlgeschlagen: HTTP {status} - {text}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_topic_names() {
        assert!(validate_topic("max_mustermann").is_ok());
        assert!(validate_topic("Team-Alerts-2").is_ok());
    }

    #[test]
    fn rejects_topics_with_path_separators_or_whitespace() {
        assert!(validate_topic("").is_err());
        assert!(validate_topic("foo/bar").is_err());
        assert!(validate_topic("foo bar").is_err());
    }

    // Beide Fälle in einem Test statt zwei separaten: NTFY_SERVER ist ein
    // Prozess-globaler Zustand, und cargo test führt Tests standardmäßig
    // parallel in Threads desselben Prozesses aus - zwei Tests, die
    // dieselbe ENV-Variable setzen/löschen, wären ein Race.
    #[test]
    fn server_from_env_defaults_and_strips_trailing_slash() {
        std::env::remove_var("NTFY_SERVER");
        std::env::remove_var("NTFY_ACCESS_TOKEN");
        assert_eq!(NtfyConfig::from_env().unwrap().server, "https://ntfy.sh");

        std::env::set_var("NTFY_SERVER", "https://ntfy.example.com/");
        assert_eq!(NtfyConfig::from_env().unwrap().server, "https://ntfy.example.com");
        std::env::remove_var("NTFY_SERVER");
    }
}
