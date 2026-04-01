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
pub struct ListLogsRequest {
    #[schemars(description = "Filter to a specific session ID")]
    pub session_id: Option<String>,

    #[schemars(description = "Filter by git branch name")]
    pub branch: Option<String>,

    #[schemars(description = "Only logs after this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub since: Option<String>,

    #[schemars(description = "Only logs before this time (e.g. '1h', '30m', '2d', or ISO 8601)")]
    pub until: Option<String>,

    #[schemars(description = "Maximum number of log lines to return (default 50)")]
    pub limit: Option<u32>,

    #[schemars(description = "Skip this many results for pagination (default 0)")]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchLogsRequest {
    #[schemars(description = "Keyword to search for in log lines (case-insensitive substring match)")]
    pub pattern: String,

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
pub struct SessionStatsRequest {
    #[schemars(description = "Session ID to get stats for (defaults to latest session if omitted)")]
    pub session_id: Option<String>,
}

// --- MCP Tool implementations ---

#[tool_router]
impl LogboxService {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Browse consecutive dev server log lines in chronological order (newest first). Use this to read what happened in a time window, see logs around a specific event, or page through recent output. Pair with search_logs: first search to find a specific event, then use list_logs with since/until to see surrounding context.")]
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
                until: req.until,
                limit: req.limit.unwrap_or(50),
                offset: req.offset.unwrap_or(0),
            },
        );
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Find dev server log lines matching a keyword (case-insensitive substring match). Use this to locate specific events like errors, status codes, or request paths. Pair with list_logs: after finding a match, use list_logs with a time range around the match timestamp to see surrounding context.")]
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
            },
        );
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| mcp_err(format!("Failed to serialize: {}", e)))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List dev server log sessions. Each session represents one run of a dev server (one invocation of `logbox collect`), tagged with git repo, branch, commit SHA, and start time. Use this to find session IDs for filtering other queries.")]
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

    #[tool(description = "Get summary stats for a dev server log session: total line count, time range, git branch, commit SHA, and repo. Defaults to the latest session if no session_id is provided.")]
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

}

#[tool_handler]
impl ServerHandler for LogboxService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "logbox captures dev server logs to SQLite so you can query them. Logs are collected by piping dev server output through `logbox collect`. Use list_logs to browse consecutive log lines in a time range or from the end. Use search_logs to find lines matching a keyword. These two tools work well together: search first to find a specific event, then list_logs with a time range around it to see context.".to_string(),
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
