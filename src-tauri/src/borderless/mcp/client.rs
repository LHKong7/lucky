//! MCP client manager — connects to MCP servers and discovers tools.

use std::collections::HashMap;

use super::protocol::{JsonRpcRequest, McpToolDescriptor};
use super::transport::McpTransport;

/// Configuration for an MCP server connection.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportType,
    /// For stdio: command to run.
    pub command: Option<String>,
    /// For stdio: command arguments.
    pub args: Vec<String>,
    /// Extra environment variables.
    pub env: HashMap<String, String>,
    /// For HTTP: endpoint URL.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransportType {
    Stdio,
    Http,
}

/// A connected MCP server with its discovered tools.
struct ConnectedServer {
    transport: Box<dyn McpTransport>,
    tools: Vec<McpToolDescriptor>,
}

/// Manages connections to MCP servers and routes tool calls.
pub struct McpManager {
    servers: HashMap<String, ConnectedServer>,
    /// Maps prefixed tool names to server names.
    tool_index: HashMap<String, String>,
    request_id: u64,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tool_index: HashMap::new(),
            request_id: 0,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.request_id += 1;
        self.request_id
    }

    /// Connect to an MCP server and discover its tools.
    pub async fn connect(&mut self, config: &McpServerConfig) -> Result<Vec<String>, String> {
        let mut transport: Box<dyn McpTransport> = match config.transport {
            McpTransportType::Stdio => {
                let cmd = config.command.as_deref().ok_or("Missing command for stdio transport")?;
                Box::new(
                    super::transport::StdioTransport::new(cmd, &config.args, &config.env).await?,
                )
            }
            McpTransportType::Http => {
                let url = config.url.as_deref().ok_or("Missing URL for HTTP transport")?;
                Box::new(super::transport::HttpTransport::new(url))
            }
        };

        // Initialize
        let init_req = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "borderless-agent",
                    "version": "0.1.0"
                }
            })),
        );
        let _ = transport.send(&init_req).await?;

        // Send initialized notification (per MCP spec, fire-and-forget)
        let init_notification = JsonRpcRequest::new(0, "notifications/initialized", None);
        let _ = transport.send(&init_notification).await;

        // Discover tools
        let list_req = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let resp = transport.send(&list_req).await?;

        let tools: Vec<McpToolDescriptor> = if let Some(result) = resp.result {
            serde_json::from_value(result["tools"].clone()).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Index tools with prefix
        let mut tool_names = Vec::new();
        for tool in &tools {
            let prefixed = format!("mcp_{}_{}", config.name, tool.name);
            self.tool_index.insert(prefixed.clone(), config.name.clone());
            tool_names.push(prefixed);
        }

        self.servers.insert(
            config.name.clone(),
            ConnectedServer { transport, tools },
        );

        Ok(tool_names)
    }

    /// Get all discovered tools in OpenAI function-calling format.
    pub fn get_tools_openai_format(&self) -> Vec<serde_json::Value> {
        let mut tools = Vec::new();
        for (server_name, server) in &self.servers {
            for tool in &server.tools {
                let prefixed_name = format!("mcp_{}_{}", server_name, tool.name);
                tools.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": prefixed_name,
                        "description": tool.description.as_deref().unwrap_or(""),
                        "parameters": tool.input_schema.clone()
                            .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
                    }
                }));
            }
        }
        tools
    }

    /// Route a tool call to the appropriate MCP server.
    pub async fn call_tool(&mut self, tool_name: &str, arguments: serde_json::Value) -> Result<String, String> {
        let server_name = self
            .tool_index
            .get(tool_name)
            .ok_or_else(|| format!("Unknown MCP tool: {}", tool_name))?
            .clone();

        // Extract the original tool name (strip prefix)
        let prefix = format!("mcp_{}_", server_name);
        let original_name = tool_name.strip_prefix(&prefix).unwrap_or(tool_name);

        let server = self
            .servers
            .get_mut(&server_name)
            .ok_or_else(|| format!("MCP server '{}' not connected", server_name))?;

        let req = JsonRpcRequest::new(
            self.request_id + 1,
            "tools/call",
            Some(serde_json::json!({
                "name": original_name,
                "arguments": arguments,
            })),
        );
        self.request_id += 1;

        let resp = server.transport.send(&req).await?;

        if let Some(error) = resp.error {
            return Err(format!("MCP tool error: {}", error.message));
        }

        Ok(resp
            .result
            .map(|r| {
                r["content"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|c| c["text"].as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default())
    }

    /// Check if a tool name belongs to an MCP server.
    pub fn is_mcp_tool(&self, tool_name: &str) -> bool {
        self.tool_index.contains_key(tool_name)
    }

    /// Close all connections.
    pub async fn close_all(&mut self) {
        for (_, mut server) in self.servers.drain() {
            let _ = server.transport.close().await;
        }
        self.tool_index.clear();
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
