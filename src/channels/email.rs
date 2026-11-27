use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use super::{Channel, ChannelDef};

pub fn channel_def() -> ChannelDef {
    ChannelDef {
        config_key: "email",
        slug: "email",
        display_name: "Email",
        validate: validate_address,
        from_env,
    }
}

fn validate_address(address: &str) -> Result<()> {
    if !address.contains('@') {
        bail!("not a valid email address ('{address}')");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(SmtpConfig::from_env()?))
}

/// Complete SMTP configuration comes from ENV, not from config.toml.
/// That keeps config.toml exclusively the contact book.
#[derive(Debug, Clone)]
struct SmtpConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    from_address: String,
    /// Display name in the "From" header, independent of the actual sender address.
    from_name: Option<String>,
    encryption: Encryption,
    timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encryption {
    StartTls,
    Tls,
    None,
}

impl SmtpConfig {
    fn from_env() -> Result<Self> {
        let host = env_required("SMTP_HOST")?;
        let port: u16 = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse()
            .context("SMTP_PORT is not a valid port number")?;
        let username = non_empty(std::env::var("SMTP_USERNAME").ok());
        let password = non_empty(std::env::var("SMTP_PASSWORD").ok());
        let from_address = env_required("SMTP_FROM_ADDRESS")?;
        let from_name = non_empty(std::env::var("SMTP_FROM_NAME").ok());

        let encryption = match std::env::var("SMTP_ENCRYPTION")
            .unwrap_or_else(|_| "starttls".to_string())
            .to_lowercase()
            .as_str()
        {
            "starttls" => Encryption::StartTls,
            "tls" => Encryption::Tls,
            "none" => Encryption::None,
            other => bail!("unknown SMTP_ENCRYPTION: '{other}' (allowed: starttls|tls|none)"),
        };

        let timeout_secs: u64 = std::env::var("SMTP_TIMEOUT_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .context("SMTP_TIMEOUT_SECS is not a valid number")?;

        if username.is_some() != password.is_some() {
            bail!("SMTP_USERNAME and SMTP_PASSWORD must either both be set or both be absent");
        }

        Ok(Self {
            host,
            port,
            username,
            password,
            from_address,
            from_name,
            encryption,
            timeout_secs,
        })
    }

    fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        let mut builder = match self.encryption {
            Encryption::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)
                    .context("could not build STARTTLS relay")?
            }
            Encryption::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)
                .context("could not build TLS relay")?,
            Encryption::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.host),
        }
        .port(self.port)
        .timeout(Some(Duration::from_secs(self.timeout_secs)));

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }

        Ok(builder.build())
    }
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("required ENV variable {key} is not set"))
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty())
}

#[async_trait]
impl Channel for SmtpConfig {
    /// Builds the connection once completely (incl. STARTTLS/TLS and AUTH,
    /// if configured) and tears it back down, without sending an email.
    async fn test_connection(&self) -> Result<()> {
        let transport = self.build_transport()?;
        let connected = transport
            .test_connection()
            .await
            .context("SMTP connection test failed")?;
        if !connected {
            bail!("SMTP connection test failed: server did not confirm NOOP");
        }
        Ok(())
    }

    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()> {
        // Mailbox::new instead of format!+parse: builds the address
        // directly from name (optional) + address, without an "@" in the
        // display name (e.g. when from_name falls back to from_address)
        // tripping up the string parser as an unquoted RFC 5322 special
        // character.
        let from_address: Address = self
            .from_address
            .parse()
            .context("SMTP_FROM_ADDRESS is not a valid sender address")?;
        let from = Mailbox::new(self.from_name.clone(), from_address);

        let to_address: Address = address
            .parse()
            .with_context(|| format!("contact email is invalid: {address}"))?;
        let to = Mailbox::new(Some(contact_name.to_string()), to_address);

        let email = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .context("could not assemble email")?;

        let transport = self.build_transport()?;
        transport
            .send(email)
            .await
            .context("SMTP send failed")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_without_display_name_is_valid() {
        // Regression: the old implementation built the From header via
        // format!("{from_display} <{addr}>").parse(), and when
        // SMTP_FROM_NAME was missing, from_display fell back to the
        // address itself - producing "test@example.com <test@example.com>",
        // an unquoted "@" in the RFC 5322 display name, which lettre's
        // Mailbox parser rejects. Only surfaced on the first real send
        // attempt (never tested live before). Mailbox::new sidesteps the
        // problem entirely, because no string needs to be parsed anymore.
        let address: Address = "test@example.com".parse().unwrap();
        let mailbox = Mailbox::new(None, address);
        assert_eq!(mailbox.to_string(), "test@example.com");
    }
}
