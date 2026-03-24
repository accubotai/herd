//! MCP Server that exposes Herd workspace capabilities to AI agents.
//!
//! # Security
//!
//! - **Stdio transport**: Safe — requires local process access.
//! - **HTTP transport**: MUST bind to `127.0.0.1` only.
//!
//! Only exposes structured process management operations.
//! Arbitrary command execution was intentionally removed.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Snapshot of a managed process for MCP queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub section: String,
    pub command: String,
}

/// Shared state that the app updates and MCP reads.
pub type SharedProcessState = Arc<Mutex<Vec<ProcessSnapshot>>>;

/// Create a new shared process state.
pub fn new_shared_state() -> SharedProcessState {
    Arc::new(Mutex::new(Vec::new()))
}

/// MCP Server for Herd workspace.
pub struct McpServer {
    transport: Transport,
    state: SharedProcessState,
}

/// Transport layer for MCP communication.
#[derive(Debug, Clone)]
pub enum Transport {
    /// Stdio transport — safe, requires local process access.
    Stdio,
    /// HTTP transport — always binds to `127.0.0.1`.
    Http {
        port: u16,
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
    pub fn new(transport: Transport, state: SharedProcessState) -> Self {
        Self { transport, state }
    }

    /// Start the MCP server.
    pub fn start(&self) -> anyhow::Result<()> {
        match &self.transport {
            Transport::Stdio => tracing::info!("MCP server ready on stdio"),
            Transport::Http { port, bind_address } => {
                tracing::info!(%port, %bind_address, "MCP server ready on HTTP");
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
            "tools/call" => self.handle_tool_call(request),
            "resources/list" => Self::handle_resources_list(),
            "resources/read" => self.handle_resource_read(request),
            "" => serde_json::json!({
                "error": { "code": -32600, "message": "Missing or invalid 'method' field" }
            }),
            _ => {
                let safe_method: String = method.chars().take(100).collect();
                serde_json::json!({
                    "error": { "code": -32601, "message": format!("Method not found: {safe_method}") }
                })
            }
        };

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

    fn handle_tool_call(&self, request: &Value) -> Value {
        let tool_name = request
            .pointer("/params/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match tool_name {
            "list_processes" | "get_process_health" => {
                let processes = self.state.lock();
                let result = serde_json::to_string_pretty(&*processes).unwrap_or_default();
                text_response(&result)
            }
            "get_process_output" => text_response("Process output retrieval not yet implemented"),
            "restart_process" | "stop_process" | "start_process" => text_response(&format!(
                "Process control via MCP not yet implemented (tool: {tool_name})"
            )),
            _ => serde_json::json!({
                "error": { "code": -32602, "message": format!("Unknown tool: {tool_name}") }
            }),
        }
    }

    fn handle_resource_read(&self, request: &Value) -> Value {
        let uri = request
            .pointer("/params/uri")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match uri {
            "process://list" => {
                let processes = self.state.lock();
                let json = serde_json::to_string(&*processes).unwrap_or_default();
                serde_json::json!({
                    "contents": [{
                        "uri": "process://list",
                        "mimeType": "application/json",
                        "text": json
                    }]
                })
            }
            _ => serde_json::json!({
                "error": { "code": -32602, "message": format!("Unknown resource: {uri}") }
            }),
        }
    }

    fn tool_definitions() -> Value {
        serde_json::json!([
            { "name": "list_processes", "description": "List all managed processes with status, PID, and resource usage", "inputSchema": { "type": "object", "properties": {} } },
            { "name": "get_process_output", "description": "Get recent terminal output from a process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" }, "lines": { "type": "integer", "default": 50 } }, "required": ["name"] } },
            { "name": "restart_process", "description": "Restart a specific managed process by name", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "stop_process", "description": "Stop a specific managed process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "start_process", "description": "Start a stopped or lazy managed process", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] } },
            { "name": "get_process_health", "description": "Health check for all processes", "inputSchema": { "type": "object", "properties": {} } }
        ])
    }
}

fn text_response(content: &str) -> Value {
    serde_json::json!({ "content": [{ "type": "text", "text": content }] })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn server() -> McpServer {
        McpServer::new(Transport::Stdio, new_shared_state())
    }

    fn request(method: &str) -> Value {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method })
    }

    #[test]
    fn test_initialize() {
        let s = server();
        let resp = s.handle_request(&request("initialize"));
        assert_eq!(resp["protocolVersion"], "2024-11-05");
        assert_eq!(resp["serverInfo"]["name"], "herd");
        assert_eq!(resp["id"], 1);
    }

    #[test]
    fn test_tools_list_returns_6_no_run_command() {
        let s = server();
        let resp = s.handle_request(&request("tools/list"));
        let tools = resp["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(!names.contains(&"run_command"));
        assert!(names.contains(&"list_processes"));
        assert!(names.contains(&"get_process_health"));
    }

    #[test]
    fn test_tool_call_list_with_state() {
        let state = new_shared_state();
        state.lock().push(ProcessSnapshot {
            name: "web".into(),
            state: "running".into(),
            pid: Some(1234),
            section: "services".into(),
            command: "npm run dev".into(),
        });
        let s = McpServer::new(Transport::Stdio, state);
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "list_processes" }
        });
        let resp = s.handle_request(&req);
        let text = resp["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("web"));
        assert!(text.contains("running"));
    }

    #[test]
    fn test_resource_read_process_list() {
        let state = new_shared_state();
        state.lock().push(ProcessSnapshot {
            name: "db".into(),
            state: "stopped".into(),
            pid: None,
            section: "services".into(),
            command: "pg_ctl".into(),
        });
        let s = McpServer::new(Transport::Stdio, state);
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "resources/read",
            "params": { "uri": "process://list" }
        });
        let resp = s.handle_request(&req);
        let text = resp["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("db"));
    }

    #[test]
    fn test_unknown_method_error() {
        let s = server();
        let resp = s.handle_request(&request("bad"));
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn test_missing_method_error() {
        let s = server();
        let resp = s.handle_request(&serde_json::json!({"id": 1}));
        assert_eq!(resp["error"]["code"], -32600);
    }

    #[test]
    fn test_unknown_tool_error() {
        let s = server();
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "evil" }
        });
        let resp = s.handle_request(&req);
        assert!(resp["error"].is_object());
    }

    #[test]
    fn test_http_localhost_only() {
        let t = Transport::http(9000);
        match t {
            Transport::Http { bind_address, .. } => {
                assert_eq!(bind_address, std::net::Ipv4Addr::LOCALHOST);
            }
            Transport::Stdio => unreachable!(),
        }
    }
}
