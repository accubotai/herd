use serde_json::Value;

/// MCP Server that exposes SoloTerm workspace capabilities to AI agents.
///
/// Implements the Model Context Protocol specification to allow
/// AI agents (Claude Code, Codex, Gemini CLI, etc.) to:
/// - Query process status
/// - Read terminal output
/// - Restart crashed services
/// - Execute commands
pub struct McpServer {
    /// Transport type (stdio or http)
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

    /// Start the MCP server
    pub async fn start(&self) -> anyhow::Result<()> {
        match &self.transport {
            Transport::Stdio => {
                tracing::info!("MCP server starting on stdio");
                // Will be implemented with rmcp crate in Phase 3
            }
            Transport::Http { port } => {
                tracing::info!(port, "MCP server starting on HTTP");
                // Will be implemented in Phase 3
            }
        }
        Ok(())
    }

    /// Handle an incoming MCP JSON-RPC request
    pub async fn handle_request(&self, request: Value) -> Value {
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        match method {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tool_call(&request).await,
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resource_read(&request).await,
            _ => serde_json::json!({
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        }
    }

    fn handle_initialize(&self) -> Value {
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "soloterm",
                "version": env!("CARGO_PKG_VERSION")
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        serde_json::json!({
            "tools": [
                {
                    "name": "list_processes",
                    "description": "List all managed processes with their status, PID, and resource usage",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_process_output",
                    "description": "Get recent terminal output from a process",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Process name" },
                            "lines": { "type": "integer", "description": "Number of lines (default 50)", "default": 50 }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "restart_process",
                    "description": "Restart a specific process by name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Process name" }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "stop_process",
                    "description": "Stop a specific process",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Process name" }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "start_process",
                    "description": "Start a stopped or lazy process",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Process name" }
                        },
                        "required": ["name"]
                    }
                },
                {
                    "name": "run_command",
                    "description": "Execute a one-off shell command and return its output",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string", "description": "Shell command to execute" },
                            "working_dir": { "type": "string", "description": "Working directory (optional)" }
                        },
                        "required": ["command"]
                    }
                },
                {
                    "name": "get_process_health",
                    "description": "Health check showing which processes are running, crashed, or stopped",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                }
            ]
        })
    }

    fn handle_resources_list(&self) -> Value {
        serde_json::json!({
            "resources": [
                {
                    "uri": "process://list",
                    "name": "Process List",
                    "description": "Live list of all managed processes with status",
                    "mimeType": "application/json"
                },
                {
                    "uri": "config://solo.toml",
                    "name": "Project Config",
                    "description": "Current solo.toml project configuration",
                    "mimeType": "application/toml"
                }
            ]
        })
    }

    async fn handle_tool_call(&self, _request: &Value) -> Value {
        // Will be wired up to Supervisor in Phase 3
        serde_json::json!({
            "content": [{
                "type": "text",
                "text": "Tool execution not yet implemented"
            }]
        })
    }

    async fn handle_resource_read(&self, _request: &Value) -> Value {
        // Will be wired up to Supervisor in Phase 3
        serde_json::json!({
            "contents": [{
                "uri": "process://list",
                "mimeType": "application/json",
                "text": "[]"
            }]
        })
    }
}
