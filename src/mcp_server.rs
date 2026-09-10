use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::{json, Map, Value};

use crate::config::Contact;
use crate::smtp::{send_mail, SmtpConfig};

/// Ein MCP-Server, der pro konfiguriertem Kontakt genau EIN Tool anbietet.
/// Es gibt bewusst KEIN generisches "send_email(to, subject, body)"-Tool —
/// die Adresse wird nie an den Agenten exponiert, nur der Name über den
/// Tool-Namen selbst.
#[derive(Clone)]
pub struct SendMailServer {
    contacts: Arc<Vec<Contact>>,
    smtp: Arc<SmtpConfig>,
}

impl SendMailServer {
    pub fn new(contacts: Vec<Contact>, smtp: SmtpConfig) -> Self {
        Self {
            contacts: Arc::new(contacts),
            smtp: Arc::new(smtp),
        }
    }

    fn find(&self, tool_name: &str) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.tool_name == tool_name)
    }

    fn tool_input_schema() -> Arc<Map<String, Value>> {
        let schema = json!({
            "type": "object",
            "properties": {
                "subject": { "type": "string", "description": "Betreff der E-Mail" },
                "body": { "type": "string", "description": "Inhalt der E-Mail (Klartext)" }
            },
            "required": ["subject", "body"]
        });
        match schema {
            Value::Object(map) => Arc::new(map),
            _ => unreachable!("json!-Makro liefert hier immer ein Objekt"),
        }
    }
}

impl ServerHandler for SendMailServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "sendmail-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Schickt E-Mails an fest konfigurierte Empfänger. Jeder Empfänger hat ein \
                 eigenes Tool (send_email_to_<name>) — es gibt kein generisches Tool mit \
                 freier Adresseingabe. Wähle das Tool, dessen Name zum gewünschten \
                 Empfänger passt, und gib subject + body an."
                    .into(),
            ),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = self
            .contacts
            .iter()
            .map(|c| {
                Tool::new(
                    c.tool_name.clone(),
                    format!("Sende eine E-Mail an {}.", c.name),
                    Self::tool_input_schema(),
                )
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let contact = self
            .find(&request.name)
            .ok_or_else(|| McpError::invalid_params(format!("unbekanntes Tool: {}", request.name), None))?;

        let args = request.arguments.unwrap_or_default();
        let subject = args
            .get("subject")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::invalid_params("Parameter 'subject' fehlt", None))?;
        let body = args
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::invalid_params("Parameter 'body' fehlt", None))?;

        match send_mail(&self.smtp, &contact.name, &contact.email, subject, body).await {
            Ok(()) => Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "E-Mail an {} wurde verschickt.",
                contact.name
            ))])),
            Err(err) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Versand an {} fehlgeschlagen: {err:#}",
                contact.name
            ))])),
        }
    }
}
