use crate::db;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct LogboxService {
    tool_router: ToolRouter<LogboxService>,
}

fn mcp_err(msg: String) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, msg, None)
}

// --- Parameter structs ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchLogsRequest {
    #[schemars(description = "Text to search for in log lines (plain text by default)")]
    pub pattern: String,

    #[schemars(description = "If true, treat pattern as a regular expression instead of plain text")]
    pub regex: Option<bool>,

    #[schemars(description = "Filter by git branch name")]
    pub branch: Option<String>,

    #[schemars(description = "Filter to a specific session ID")]
    pub session_id: Option<String>,

    #[schemars(description = "Only logs after this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub since: Option<String>,

    #[schemars(description = "Only logs before this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub until: Option<String>,

    #[schemars(description = "Maximum number of results (default 100)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSessionsRequest {
    #[schemars(description = "Filter by git branch name")]
    pub branch: Option<String>,

    #[schemars(description = "Filter by commit SHA (prefix match)")]
    pub commit: Option<String>,

    #[schemars(description = "Only sessions started after this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub since: Option<String>,

    #[schemars(description = "Maximum number of results (default 20)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListLogsRequest {
    #[schemars(description = "Filter to a specific session ID")]
    pub session_id: Option<String>,

    #[schemars(description = "Filter by git branch name")]
    pub branch: Option<String>,

    #[schemars(description = "Only logs after this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub since: Option<String>,

    #[schemars(description = "Maximum number of results (default 50)")]
    pub limit: Option<u32>,

    #[schemars(description = "Skip this many results for pagination (default 0)")]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SessionStatsRequest {
    #[schemars(description = "Session ID to get stats for (defaults to latest session if omitted)")]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompareSessionsRequest {
    #[schemars(description = "First session ID")]
    pub session_a: String,

    #[schemars(description = "Second session ID")]
    pub session_b: String,

    #[schemars(description = "Optional text pattern to filter lines before comparing")]
    pub pattern: Option<String>,

    #[schemars(description = "If true, treat pattern as a regular expression instead of plain text")]
    pub regex: Option<bool>,
}

// --- MCP Tool implementations ---

#[tool_router]
impl LogboxService {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "List recent dev server logs (newest first). Use this to browse logs without a search pattern. Supports pagination with offset.")]
    fn list_logs(
        &self,
        Parameters(req): Parameters<ListLogsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = db::open_db().map_err(|e| mcp_err(format!("Failed to open database: {}", e)))?;
        let results = db::list_logs(
            &conn,
            &db::LogsFilters {
                session_id: req.session_id,
                branch: req.branch,
                since: req.since,
                limit: req.limit.unwrap_or(50),
                offset: req.offset.unwrap_or(0),
            },
        );
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Search dev server logs by keyword or pattern. Plain text search by default — set regex=true for regular expressions. Returns matching log lines with timestamps and session info.")]
    fn search_logs(
        &self,
        Parameters(req): Parameters<SearchLogsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = db::open_db().map_err(|e| mcp_err(format!("Failed to open database: {}", e)))?;
        let results = db::search_logs(
            &conn,
            &req.pattern,
            &db::SearchFilters {
                branch: req.branch,
                session_id: req.session_id,
                since: req.since,
                until: req.until,
                limit: req.limit.unwrap_or(100),
                is_regex: req.regex.unwrap_or(false),
            },
        );
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List recorded dev server log sessions. Each session represents one run of a dev server, tagged with git branch, commit SHA, and repo.")]
    fn list_sessions(
        &self,
        Parameters(req): Parameters<ListSessionsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = db::open_db().map_err(|e| mcp_err(format!("Failed to open database: {}", e)))?;
        let results = db::list_sessions(
            &conn,
            &db::SessionFilters {
                branch: req.branch,
                commit: req.commit,
                since: req.since,
                limit: req.limit.unwrap_or(20),
            },
        );
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Get stats for a dev server log session including line count, time range, and git info. Defaults to the latest session if no session_id is provided.")]
    fn session_stats(
        &self,
        Parameters(req): Parameters<SessionStatsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = db::open_db().map_err(|e| mcp_err(format!("Failed to open database: {}", e)))?;
        match db::session_stats(&conn, req.session_id.as_deref()) {
            Some(stats) => {
                let json = serde_json::to_string_pretty(&stats)
                    .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "No session found.".to_string(),
            )])),
        }
    }

    #[tool(description = "Compare logs between two dev server sessions. Shows how many lines are unique to each session and how many are common. Useful for comparing before/after a code change.")]
    fn compare_sessions(
        &self,
        Parameters(req): Parameters<CompareSessionsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = db::open_db().map_err(|e| mcp_err(format!("Failed to open database: {}", e)))?;
        match db::compare_sessions(&conn, &req.session_a, &req.session_b, req.pattern.as_deref(), req.regex.unwrap_or(false)) {
            Some(result) => {
                let json = serde_json::to_string_pretty(&result)
                    .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(
                "One or both sessions not found.".to_string(),
            )])),
        }
    }
}

#[tool_handler]
impl ServerHandler for LogboxService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "logbox persists dev server logs to SQLite. Use these tools to search logs, list sessions, get stats, and compare sessions. Logs are captured by piping dev server output through `logbox collect`.".to_string(),
            ),
        }
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    let service = LogboxService::new();
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
