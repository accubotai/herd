//! MCP Server that exposes Herd workspace capabilities to AI agents.
//!
//! # Security
//!
//! - **Stdio transport**: Safe — requires local process access.
//! - **HTTP transport**: MUST bind to `127.0.0.1` only. Authentication is required
//!   before this transport can be used in production. See `Transport::Http`.
//!
//! The server only exposes structured process management operations (start, stop,
//! restart, status). Arbitrary command execution (`run_command`) was intentionally
//! removed — it would be an unauthenticated RCE vector.

use serde_json::Value;

/// MCP Server for Herd workspace.
pub struct McpServer {
    transport: Transport,
}

/// Transport layer for MCP communication.
#[derive(Debug, Clone)]
pub enum Transport {
    /// Stdio transport — safe, requires local process access.
    Stdio,
    /// HTTP transport — **SECURITY**: always binds to 127.0.0.1.
    /// Authentication must be implemented before enabling remote access.
    Http {
        port: u16,
        /// Always 127.0.0.1 — never expose to network without auth.
        bind_address: std::net::Ipv4Addr,
    },
}

impl Transport {
    /// Create an HTTP transport bound to localhost only.
    pub fn http(port: u16) -> Self {
        Self::Http {
            port,
            bind_address: std::net::Ipv4Addr::LOCALHOST,
        }
    }
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
            Transport::Http { port, bind_address } => {
                tracing::info!(%port, %bind_address, "MCP server starting on HTTP");
            }
        }
        Ok(())
    }

    /// Handle an incoming MCP JSON-RPC request.
    pub fn handle_request(&self, request: &Value) -> Value {
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = request.get("id").cloned();

        let mut response = match method {
            "initialize" => Self::handle_initialize(),
            "tools/list" => Self::handle_tools_list(),
            "tools/call" => Self::handle_tool_call(request),
            "resources/list" => Self::handle_resources_list(),
            "resources/read" => Self::handle_resource_read(request),
            "" => serde_json::json!({
                "error": { "code": -32600, "message": "Missing or invalid 'method' field" }
            }),
            _ => {
                // Truncate method name to prevent log/response injection
                let safe_method: String = method.chars().take(100).collect();
                serde_json::json!({
                    "error": { "code": -32601, "message": format!("Method not found: {safe_method}") }
                })
            }
        };

        // Propagate JSON-RPC id for proper response routing
        if let (Some(id_val), Some(obj)) = (id, response.as_object_mut()) {
            obj.insert("id".to_string(), id_val);
        }

        response
    }

    fn handle_initialize() -> Value {
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {}, "resources": {} },
            "serverInfo": { "name": "herd", "version": env!("CARGO_PKG_VERSION") }
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

    /// Tool definitions — only structured process management operations.
    ///
    /// `run_command` was intentionally removed. Arbitrary command execution
    /// through MCP is an unauthenticated RCE vector. If needed in the future,
    /// it must require: (1) authentication, (2) an allowlist, (3) rate limiting.
    fn tool_definitions() -> Value {
        serde_json::json!([
            {
                "name": "list_processes",
                "description": "List all managed processes with status, PID, and resource usage",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_process_output",
                "description": "Get recent terminal output from a process",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Process name" },
                        "lines": { "type": "integer", "description": "Number of lines", "default": 50 }
                    },
                    "required": ["name"]
                }
            },
            {
                "name": "restart_process",
                "description": "Restart a specific managed process by name",
                "inputSchema": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
                }
            },
            {
                "name": "stop_process",
                "description": "Stop a specific managed process",
                "inputSchema": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
                }
            },
            {
                "name": "start_process",
                "description": "Start a stopped or lazy managed process",
                "inputSchema": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
                }
            },
            {
                "name": "get_process_health",
                "description": "Health check showing which processes are running, crashed, or stopped",
                "inputSchema": { "type": "object", "properties": {} }
            }
        ])
    }
}
