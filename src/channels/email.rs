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
        display_name: "E-Mail",
        validate: validate_address,
        from_env,
    }
}

fn validate_address(address: &str) -> Result<()> {
    if !address.contains('@') {
        bail!("keine gültige E-Mail-Adresse ('{address}')");
    }
    Ok(())
}

fn from_env() -> Result<Box<dyn Channel>> {
    Ok(Box::new(SmtpConfig::from_env()?))
}

/// Komplette SMTP-Konfiguration kommt aus ENV, nicht aus der config.toml.
/// Die config.toml bleibt dadurch ausschließlich das Kontaktbuch.
#[derive(Debug, Clone)]
struct SmtpConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    from_address: String,
    /// Anzeigename im "From"-Header, unabhängig von der tatsächlichen Absenderadresse.
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

#[async_trait]
impl Channel for SmtpConfig {
    /// Baut die Verbindung einmal komplett auf (inkl. STARTTLS/TLS und AUTH,
    /// falls konfiguriert) und wieder ab, ohne eine Mail zu verschicken.
    async fn test_connection(&self) -> Result<()> {
        let transport = self.build_transport()?;
        let connected = transport
            .test_connection()
            .await
            .context("SMTP-Verbindungstest fehlgeschlagen")?;
        if !connected {
            bail!("SMTP-Verbindungstest fehlgeschlagen: Server hat NOOP nicht bestätigt");
        }
        Ok(())
    }

    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()> {
        // Mailbox::new statt format!+parse: baut die Adresse direkt aus
        // Name (optional) + Address zusammen, ohne dass ein "@" im
        // Anzeigenamen (z.B. wenn from_name auf from_address zurückfällt)
        // als unquotiertes RFC-5322-Sonderzeichen den String-Parser stolpern
        // lässt.
        let from_address: Address = self
            .from_address
            .parse()
            .context("SMTP_FROM_ADDRESS ist keine gültige Absenderadresse")?;
        let from = Mailbox::new(self.from_name.clone(), from_address);

        let to_address: Address = address
            .parse()
            .with_context(|| format!("Kontakt-E-Mail ist ungültig: {address}"))?;
        let to = Mailbox::new(Some(contact_name.to_string()), to_address);

        let email = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .context("konnte E-Mail nicht zusammenbauen")?;

        let transport = self.build_transport()?;
        transport
            .send(email)
            .await
            .context("SMTP-Versand fehlgeschlagen")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_without_display_name_is_valid() {
        // Regression: die alte Implementierung baute den From-Header über
        // format!("{from_display} <{addr}>").parse(), und wenn SMTP_FROM_NAME
        // fehlte, fiel from_display auf die Adresse selbst zurück - macht
        // "test@example.com <test@example.com>", ein unquotiertes "@" im
        // RFC-5322-Anzeigenamen, das lettres Mailbox-Parser ablehnt. Erst
        // beim ersten echten Sendeversuch aufgefallen (vorher nie live
        // getestet). Mailbox::new umgeht das Problem komplett, weil kein
        // String mehr geparst werden muss.
        let address: Address = "test@example.com".parse().unwrap();
        let mailbox = Mailbox::new(None, address);
        assert_eq!(mailbox.to_string(), "test@example.com");
    }
}
