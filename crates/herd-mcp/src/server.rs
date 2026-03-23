use serde_json::Value;

/// MCP Server that exposes `Herd` workspace capabilities to AI agents.
///
/// Implements the Model Context Protocol specification to allow
/// AI agents (Claude Code, Codex, Gemini CLI, etc.) to:
/// - Query process status
/// - Read terminal output
/// - Restart crashed services
/// - Execute commands
pub struct McpServer {
    transport: Transport,
}

#[derive(Debug, Clone)]
pub enum Transport {
    Stdio,
    Http { port: u16 },
}

impl McpServer {
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }

    /// Start the MCP server.
    pub fn start(&self) -> anyhow::Result<()> {
        match &self.transport {
            Transport::Stdio => {
                tracing::info!("MCP server starting on stdio");
            }
            Transport::Http { port } => {
                tracing::info!(port, "MCP server starting on HTTP");
            }
        }
        Ok(())
    }

    /// Handle an incoming MCP JSON-RPC request.
    pub fn handle_request(&self, request: &Value) -> Value {
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "initialize" => Self::handle_initialize(),
            "tools/list" => Self::handle_tools_list(),
            "tools/call" => Self::handle_tool_call(request),
            "resources/list" => Self::handle_resources_list(),
            "resources/read" => Self::handle_resource_read(request),
            _ => serde_json::json!({
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {method}")
                }
            }),
        }
    }

    fn handle_initialize() -> Value {
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {}, "resources": {} },
            "serverInfo": {
                "name": "herd",
                "version": env!("CARGO_PKG_VERSION")
            }
        })
    }

    fn handle_tools_list() -> Value {
        serde_json::json!({ "tools": Self::tool_definitions() })
    }

    fn handle_resources_list() -> Value {
        serde_json::json!({
            "resources": [
                {
                    "uri": "process://list",
                    "name": "Process List",
                    "description": "Live list of all managed processes with status",
                    "mimeType": "application/json"
                },
                {
                    "uri": "config://herd.toml",
                    "name": "Project Config",
                    "description": "Current herd.toml project configuration",
                    "mimeType": "application/toml"
                }
            ]
        })
    }

    fn handle_tool_call(_request: &Value) -> Value {
        serde_json::json!({
            "content": [{ "type": "text", "text": "Tool execution not yet implemented" }]
        })
    }

    fn handle_resource_read(_request: &Value) -> Value {
        serde_json::json!({
            "contents": [{
                "uri": "process://list",
                "mimeType": "application/json",
                "text": "[]"
            }]
        })
    }

    fn tool_definitions() -> Value {
        serde_json::json!([
            { "name": "list_processes", "description": "List all managed processes with status, PID, and resource usage", "inputSchema": { "type": "object", "properties": {} } },
            { "name": "get_process_output", "description": "Get recent terminal output from a process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" }, "lines": { "type": "integer", "default": 50 } }, "required": ["name"] } },
            { "name": "restart_process", "description": "Restart a specific process by name", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "stop_process", "description": "Stop a specific process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "start_process", "description": "Start a stopped or lazy process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "run_command", "description": "Execute a one-off shell command and return output", "inputSchema": { "type": "object", "properties": { "command": { "type": "string" }, "working_dir": { "type": "string" } }, "required": ["command"] } },
            { "name": "get_process_health", "description": "Health check for all processes", "inputSchema": { "type": "object", "properties": {} } }
        ])
    }
}
