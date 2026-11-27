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

/// ntfy topics are effectively the secret (whoever knows the topic name
/// can read along) - the allowed character set is deliberately
/// conservative, so no value can accidentally mess up the URL path
/// structure ("/") or HTTP headers (whitespace).
fn validate_topic(topic: &str) -> Result<()> {
    let valid = !topic.is_empty()
        && topic
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        bail!("not a valid ntfy topic ('{topic}') - only letters, digits, '_' and '-' are allowed");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(NtfyConfig::from_env()?))
}

/// Server + optional access token come from ENV, never from config.toml.
#[derive(Debug, Clone)]
struct NtfyConfig {
    /// Without a trailing "/", e.g. "https://ntfy.sh" (default) or a
    /// self-hosted instance.
    server: String,
    /// Only needed if the server/topic requires access control - the
    /// public ntfy.sh service usually doesn't need one.
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
    /// ntfy has no per-topic auth check (topics are implicitly created on
    /// first publish) - `test_connection` instead only checks whether the
    /// server is reachable and healthy (`GET /v1/health`).
    async fn test_connection(&self) -> Result<()> {
        let url = format!("{}/v1/health", self.server);
        let response = self
            .authorize(self.client().get(&url))
            .send()
            .await
            .context("ntfy connection test failed (network)")?;
        let status = response.status();
        if !status.is_success() {
            bail!("ntfy connection test failed: HTTP {status}");
        }
        let health: HealthResponse = response.json().await.context("ntfy health response not readable")?;
        if !health.healthy {
            bail!("ntfy connection test failed: server does not report itself as 'healthy'");
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
            .context("ntfy send failed (network)")?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            bail!("ntfy send failed: HTTP {status} - {text}");
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

    // Both cases in one test instead of two separate ones: NTFY_SERVER is
    // process-global state, and cargo test runs tests in parallel threads
    // of the same process by default - two tests setting/removing the
    // same ENV variable would race.
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
