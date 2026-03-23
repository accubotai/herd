//! MCP resource URI definitions.
//!
//! Resources provide read-only access to workspace state
//! that AI agents can query for context.

pub const RESOURCE_PROCESS_LIST: &str = "process://list";
pub const RESOURCE_PROJECT_CONFIG: &str = "config://herd.toml";

/// Dynamic resource URI pattern for per-process output
pub fn process_output_uri(name: &str) -> String {
    format!("process://{name}/output")
}
