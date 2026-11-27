use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::{json, Map, Value};

use crate::channels::Channel;
use crate::config::ContactEntry;

/// A channel that is actually in use (has at least one contact in
/// config.toml and a working sender), and therefore gets its own pair of
/// tools.
#[derive(Debug, Clone, Copy)]
pub struct ActiveChannel {
    pub slug: &'static str,
    pub display_name: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum ToolKind {
    /// `send_via_<channel>` - contact is a parameter value, resolved
    /// against the live contact book.
    Send(&'static str),
    /// `list_<channel>_contacts` - lets the agent discover currently valid
    /// `contact` values for that channel without needing a fresh
    /// `tools/list`.
    List(&'static str),
}

/// An MCP server that offers exactly TWO tools per active channel:
/// `send_via_<channel>` and `list_<channel>_contacts`. The tool set itself
/// is fixed at startup (determined by which channels are configured at
/// all) and never changes; only the contact data behind it is reloaded
/// live from config.toml (see `crate::reload`). There is no generic
/// `send_message(channel, to, ...)` tool, and `contact` is never a raw
/// address, chat ID, or webhook URL - it is a slug that is validated
/// against the live contact book on every call and rejected outright if
/// unknown (see docs/security-model.md).
#[derive(Clone)]
pub struct SendMailServer {
    contacts: Arc<RwLock<HashMap<&'static str, Vec<ContactEntry>>>>,
    senders: Arc<HashMap<&'static str, Box<dyn Channel>>>,
    tools: Arc<Vec<Tool>>,
    dispatch: Arc<HashMap<String, ToolKind>>,
}

impl SendMailServer {
    pub fn new(
        active_channels: Vec<ActiveChannel>,
        contacts: Arc<RwLock<HashMap<&'static str, Vec<ContactEntry>>>>,
        senders: HashMap<&'static str, Box<dyn Channel>>,
    ) -> Self {
        let mut tools = Vec::new();
        let mut dispatch = HashMap::new();

        for ch in &active_channels {
            let send_name = format!("send_via_{}", ch.slug);
            let list_name = format!("list_{}_contacts", ch.slug);

            tools.push(Tool {
                title: Some(ch.display_name.to_string()),
                ..Tool::new(
                    send_name.clone(),
                    format!(
                        "Send a message via {}. 'contact' must be one of the slugs returned by \
                         the '{list_name}' tool - an unknown value is rejected, never used as a \
                         literal address, chat ID, or webhook URL.",
                        ch.display_name
                    ),
                    Self::send_schema(),
                )
            });
            tools.push(Tool {
                title: Some(format!("{} Contacts", ch.display_name)),
                ..Tool::new(
                    list_name.clone(),
                    format!(
                        "List the contacts currently configured for {} (name + slug). Call this \
                         before 'send_via_{}' to see which 'contact' values are valid right now.",
                        ch.display_name, ch.slug
                    ),
                    Self::list_schema(),
                )
            });

            dispatch.insert(send_name, ToolKind::Send(ch.slug));
            dispatch.insert(list_name, ToolKind::List(ch.slug));
        }

        Self {
            contacts,
            senders: Arc::new(senders),
            tools: Arc::new(tools),
            dispatch: Arc::new(dispatch),
        }
    }

    fn send_schema() -> Arc<Map<String, Value>> {
        let schema = json!({
            "type": "object",
            "properties": {
                "contact": {
                    "type": "string",
                    "description": "Contact slug for this channel - see the matching list_<channel>_contacts tool for currently valid values"
                },
                "subject": { "type": "string", "description": "Subject/title of the message" },
                "body": { "type": "string", "description": "Content of the message (plain text)" }
            },
            "required": ["contact", "subject", "body"]
        });
        match schema {
            Value::Object(map) => Arc::new(map),
            _ => unreachable!("the json! macro always produces an object here"),
        }
    }

    fn list_schema() -> Arc<Map<String, Value>> {
        let schema = json!({ "type": "object", "properties": {} });
        match schema {
            Value::Object(map) => Arc::new(map),
            _ => unreachable!("the json! macro always produces an object here"),
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
                "Sends messages over fixed, pre-configured channels. Each channel has exactly \
                 two tools: 'list_<channel>_contacts' (no parameters) returns the contacts \
                 currently configured for that channel, and 'send_via_<channel>' sends to one of \
                 them via the 'contact' parameter. Always call 'list_<channel>_contacts' first to \
                 see which 'contact' values are valid - an unknown value is rejected and never \
                 used as a literal address, chat ID, or webhook URL."
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
        Ok(ListToolsResult::with_all_items((*self.tools).clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let kind = *self
            .dispatch
            .get(&request.name as &str)
            .ok_or_else(|| McpError::invalid_params(format!("unknown tool: {}", request.name), None))?;

        match kind {
            ToolKind::List(channel_slug) => {
                let contacts = self.contacts.read().expect("lock not poisoned");
                let entries = contacts.get(channel_slug).map(Vec::as_slice).unwrap_or(&[]);
                let text = if entries.is_empty() {
                    "No contacts configured for this channel.".to_string()
                } else {
                    entries
                        .iter()
                        .map(|e| format!("{} ({})", e.contact_slug, e.contact_name))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            ToolKind::Send(channel_slug) => {
                let args = request.arguments.unwrap_or_default();
                let contact_slug = args
                    .get("contact")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::invalid_params("parameter 'contact' is missing", None))?;
                let subject = args
                    .get("subject")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::invalid_params("parameter 'subject' is missing", None))?;
                let body = args
                    .get("body")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::invalid_params("parameter 'body' is missing", None))?;

                let (entry, valid_slugs) = {
                    let contacts = self.contacts.read().expect("lock not poisoned");
                    let entries = contacts.get(channel_slug).map(Vec::as_slice).unwrap_or(&[]);
                    let entry = entries.iter().find(|e| e.contact_slug == contact_slug).cloned();
                    let valid_slugs: Vec<String> = entries.iter().map(|e| e.contact_slug.clone()).collect();
                    (entry, valid_slugs)
                };
                let Some(entry) = entry else {
                    return Err(McpError::invalid_params(
                        format!(
                            "unknown contact '{contact_slug}' for this channel. Currently valid: {}",
                            if valid_slugs.is_empty() {
                                "none configured".to_string()
                            } else {
                                valid_slugs.join(", ")
                            }
                        ),
                        None,
                    ));
                };

                let sender = self
                    .senders
                    .get(channel_slug)
                    .expect("sender always exists for an active channel");

                match sender.send(&entry.contact_name, &entry.address, subject, body).await {
                    Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Message to {} was sent.",
                        entry.contact_name
                    ))])),
                    Err(err) => Ok(CallToolResult::error(vec![Content::text(format!(
                        "Sending to {} failed: {err:#}",
                        entry.contact_name
                    ))])),
                }
            }
        }
    }
}
