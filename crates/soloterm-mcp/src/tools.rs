/// MCP tool definitions and handlers.
///
/// These will be wired up to the Supervisor in Phase 3.
/// Each tool corresponds to a process management action
/// that AI agents can invoke.

/// Tool names as constants for consistent referencing
pub const TOOL_LIST_PROCESSES: &str = "list_processes";
pub const TOOL_GET_PROCESS_OUTPUT: &str = "get_process_output";
pub const TOOL_RESTART_PROCESS: &str = "restart_process";
pub const TOOL_STOP_PROCESS: &str = "stop_process";
pub const TOOL_START_PROCESS: &str = "start_process";
pub const TOOL_RUN_COMMAND: &str = "run_command";
pub const TOOL_GET_PROCESS_HEALTH: &str = "get_process_health";
pub const TOOL_GET_PROJECT_CONFIG: &str = "get_project_config";
