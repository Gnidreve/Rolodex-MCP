use std::time::Duration;

use anyhow::{bail, Context, Result};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Komplette SMTP-Konfiguration kommt aus ENV, nicht aus der config.toml.
/// Die config.toml bleibt dadurch ausschließlich das Kontaktbuch.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
    /// Anzeigename im "From"-Header, unabhängig von der tatsächlichen Absenderadresse.
    pub from_name: Option<String>,
    pub encryption: Encryption,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    StartTls,
    Tls,
    None,
}

impl SmtpConfig {
    pub fn from_env() -> Result<Self> {
        let host = env_required("SMTP_HOST")?;
        let port: u16 = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse()
            .context("SMTP_PORT ist keine gültige Portnummer")?;
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
            other => bail!("SMTP_ENCRYPTION unbekannt: '{other}' (erlaubt: starttls|tls|none)"),
        };

        let timeout_secs: u64 = std::env::var("SMTP_TIMEOUT_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .context("SMTP_TIMEOUT_SECS ist keine gültige Zahl")?;

        if username.is_some() != password.is_some() {
            bail!("SMTP_USERNAME und SMTP_PASSWORD müssen entweder beide gesetzt sein oder beide fehlen");
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
                    .context("konnte STARTTLS-Relay nicht aufbauen")?
            }
            Encryption::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)
                .context("konnte TLS-Relay nicht aufbauen")?,
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
    std::env::var(key).with_context(|| format!("Pflicht-ENV-Variable {key} ist nicht gesetzt"))
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty())
}

pub async fn send_mail(
    cfg: &SmtpConfig,
    to_name: &str,
    to_email: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    let from_display = cfg
        .from_name
        .clone()
        .unwrap_or_else(|| cfg.from_address.clone());

    let from = format!("{from_display} <{}>", cfg.from_address)
        .parse()
        .context("SMTP_FROM_NAME/SMTP_FROM_ADDRESS ergeben keine gültige Absenderadresse")?;
    let to = format!("{to_name} <{to_email}>")
        .parse()
        .with_context(|| format!("Kontakt-E-Mail ist ungültig: {to_email}"))?;

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .context("konnte E-Mail nicht zusammenbauen")?;

    let transport = cfg.build_transport()?;
    transport
        .send(email)
        .await
        .context("SMTP-Versand fehlgeschlagen")?;

    Ok(())
}
