use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::{json, Map, Value};

use crate::channels::Channel;
use crate::config::ChannelTool;

/// Ein MCP-Server, der pro konfiguriertem Kontakt UND Kanal genau EIN Tool
/// anbietet. Es gibt bewusst KEIN generisches "send_message(channel, to,
/// subject, body)"-Tool — weder die Adresse/Chat-ID noch der Kanal werden
/// dem Agenten als Parameter exponiert, beides steckt fest im Tool-Namen
/// selbst (z.B. "send_to_max_mustermann_via_telegram"). Diese Datei kennt
/// keinen einzigen konkreten Kanal - nur den `Channel`-Trait.
#[derive(Clone)]
pub struct SendMailServer {
    tools: Arc<Vec<ChannelTool>>,
    senders: Arc<HashMap<&'static str, Box<dyn Channel>>>,
}

impl SendMailServer {
    pub fn new(tools: Vec<ChannelTool>, senders: HashMap<&'static str, Box<dyn Channel>>) -> Self {
        Self {
            tools: Arc::new(tools),
            senders: Arc::new(senders),
        }
    }

    fn find(&self, tool_name: &str) -> Option<&ChannelTool> {
        self.tools.iter().find(|t| t.tool_name == tool_name)
    }

    fn tool_input_schema() -> Arc<Map<String, Value>> {
        let schema = json!({
            "type": "object",
            "properties": {
                "subject": { "type": "string", "description": "Betreff/Titel der Nachricht" },
                "body": { "type": "string", "description": "Inhalt der Nachricht (Klartext)" }
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
                ..Default::default()
            },
            instructions: Some(
                "Schickt Nachrichten an fest konfigurierte Empfänger, über fest konfigurierte \
                 Kanäle. Jede Empfänger-Kanal-Kombination hat ein eigenes Tool \
                 (send_to_<name>_via_<kanal>) — es gibt kein generisches Tool mit freier \
                 Adresseingabe oder Kanalwahl. Wähle das Tool, dessen Name zu gewünschtem \
                 Empfänger UND Kanal passt, und gib subject + body an."
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
            .tools
            .iter()
            .map(|t| {
                let description = format!("Sende eine Nachricht an {} über {}.", t.contact_name, t.channel_display_name);
                Tool {
                    title: Some(t.title.clone()),
                    ..Tool::new(t.tool_name.clone(), description, Self::tool_input_schema())
                }
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool = self
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

        let sender = self
            .senders
            .get(tool.channel_slug)
            .expect("Sender existiert immer, weil das Tool nur für konfigurierte Kanäle erzeugt wird");

        match sender.send(&tool.contact_name, &tool.address, subject, body).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Nachricht an {} wurde verschickt.",
                tool.contact_name
            ))])),
            Err(err) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Versand an {} fehlgeschlagen: {err:#}",
                tool.contact_name
            ))])),
        }
    }
}
