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

/// An MCP server that offers exactly ONE tool per configured contact AND
/// channel. There is deliberately NO generic "send_message(channel, to,
/// subject, body)" tool — neither the address/chat ID nor the channel are
/// exposed to the agent as a parameter, both are baked into the tool name
/// itself (e.g. "send_to_max_mustermann_via_telegram"). This file doesn't
/// know a single concrete channel - only the `Channel` trait.
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
                "subject": { "type": "string", "description": "Subject/title of the message" },
                "body": { "type": "string", "description": "Content of the message (plain text)" }
            },
            "required": ["subject", "body"]
        });
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
                "Sends messages to fixed, pre-configured recipients, over fixed, pre-configured \
                 channels. Every recipient-channel combination has its own tool \
                 (send_to_<name>_via_<channel>) — there is no generic tool with a free-form \
                 address or channel choice. Pick the tool whose name matches the desired \
                 recipient AND channel, and provide subject + body."
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
                let description = format!("Send a message to {} via {}.", t.contact_name, t.channel_display_name);
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
            .ok_or_else(|| McpError::invalid_params(format!("unknown tool: {}", request.name), None))?;

        let args = request.arguments.unwrap_or_default();
        let subject = args
            .get("subject")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::invalid_params("parameter 'subject' is missing", None))?;
        let body = args
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::invalid_params("parameter 'body' is missing", None))?;

        let sender = self
            .senders
            .get(tool.channel_slug)
            .expect("sender always exists, because the tool is only generated for configured channels");

        match sender.send(&tool.contact_name, &tool.address, subject, body).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Message to {} was sent.",
                tool.contact_name
            ))])),
            Err(err) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Sending to {} failed: {err:#}",
                tool.contact_name
            ))])),
        }
    }
}
