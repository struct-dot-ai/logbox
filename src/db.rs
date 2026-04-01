use chrono::{DateTime, Duration, Local, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::PathBuf;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    repo TEXT,
    branch TEXT,
    commit_sha TEXT,
    started_at TEXT NOT NULL,
    cwd TEXT
);

CREATE TABLE IF NOT EXISTS logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    timestamp TEXT NOT NULL,
    line TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_logs_session ON logs(session_id);
CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_sessions_branch ON sessions(branch);
"#;

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub id: i64,
    pub session_id: String,
    pub timestamp: String,
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub started_at: String,
    pub last_log_at: Option<String>,
    pub cwd: Option<String>,
    pub log_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub session_id: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub started_at: String,
    pub total_lines: i64,
    pub first_log: Option<String>,
    pub last_log: Option<String>,
}

pub struct SearchFilters {
    pub branch: Option<String>,
    pub session_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: u32,
}

pub struct LogsFilters {
    pub session_id: Option<String>,
    pub branch: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

pub struct SessionFilters {
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub since: Option<String>,
    pub limit: u32,
}

fn db_path() -> PathBuf {
    let home = dirs_home().expect("Could not determine home directory");
    home.join(".logbox").join("logs.db")
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Open an in-memory database for testing
#[cfg(test)]
pub fn open_memory_db() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn open_db() -> rusqlite::Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create ~/.logbox directory");
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn create_session(
    conn: &Connection,
    id: &str,
    repo: Option<&str>,
    branch: Option<&str>,
    commit_sha: Option<&str>,
    cwd: Option<&str>,
) -> rusqlite::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, repo, branch, commit_sha, started_at, cwd) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, repo, branch, commit_sha, now, cwd],
    )?;
    Ok(())
}

pub fn insert_logs_batch(
    conn: &Connection,
    session_id: &str,
    lines: &[(String, String)], // (timestamp, line)
) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO logs (session_id, timestamp, line) VALUES (?1, ?2, ?3)",
        )?;
        for (ts, line) in lines {
            stmt.execute(params![session_id, ts, line])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Parse a relative time string like "1h", "30m", "2d" into an ISO 8601 timestamp
fn parse_since(since: &str) -> Option<String> {
    let since = since.trim();

    // Try parsing as ISO 8601 first
    if DateTime::parse_from_rfc3339(since).is_ok() {
        return Some(since.to_string());
    }

    // Parse relative time
    let (num_str, unit) = since.split_at(since.len().saturating_sub(1));
    let num: i64 = num_str.parse().ok()?;
    let duration = match unit {
        "s" => Duration::seconds(num),
        "m" => Duration::minutes(num),
        "h" => Duration::hours(num),
        "d" => Duration::days(num),
        _ => return None,
    };

    let ts = Local::now() - duration;
    Some(ts.to_rfc3339())
}

pub fn list_logs(conn: &Connection, filters: &LogsFilters) -> Vec<LogEntry> {
    let mut sql = String::from(
        "SELECT l.id, l.session_id, l.timestamp, l.line, s.branch \
         FROM logs l JOIN sessions s ON l.session_id = s.id WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

    if let Some(ref session_id) = filters.session_id {
        sql.push_str(" AND l.session_id = ?");
        param_values.push(Box::new(session_id.clone()));
    }
    if let Some(ref branch) = filters.branch {
        sql.push_str(" AND s.branch = ?");
        param_values.push(Box::new(branch.clone()));
    }
    if let Some(ref since) = filters.since {
        if let Some(ts) = parse_since(since) {
            sql.push_str(" AND l.timestamp >= ?");
            param_values.push(Box::new(ts));
        }
    }
    if let Some(ref until) = filters.until {
        if let Some(ts) = parse_since(until) {
            sql.push_str(" AND l.timestamp <= ?");
            param_values.push(Box::new(ts));
        }
    }

    sql.push_str(" ORDER BY l.timestamp DESC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", filters.limit, filters.offset));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SQL error: {}", e);
            return vec![];
        }
    };

    stmt.query_map(params_refs.as_slice(), |row| {
        Ok(LogEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            timestamp: row.get(2)?,
            line: row.get(3)?,
            branch: row.get(4)?,
        })
    })
    .unwrap_or_else(|e| {
        eprintln!("Query error: {}", e);
        panic!("Query failed");
    })
    .filter_map(|r| r.ok())
    .collect()
}

pub fn search_logs(conn: &Connection, pattern: &str, filters: &SearchFilters) -> Vec<LogEntry> {
    let mut sql = String::from(
        "SELECT l.id, l.session_id, l.timestamp, l.line, s.branch \
         FROM logs l JOIN sessions s ON l.session_id = s.id WHERE l.line LIKE ?",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
        Box::new(format!("%{}%", pattern)),
    ];

    if let Some(ref branch) = filters.branch {
        sql.push_str(" AND s.branch = ?");
        param_values.push(Box::new(branch.clone()));
    }
    if let Some(ref session_id) = filters.session_id {
        sql.push_str(" AND l.session_id = ?");
        param_values.push(Box::new(session_id.clone()));
    }
    if let Some(ref since) = filters.since {
        if let Some(ts) = parse_since(since) {
            sql.push_str(" AND l.timestamp >= ?");
            param_values.push(Box::new(ts));
        }
    }
    if let Some(ref until) = filters.until {
        if let Some(ts) = parse_since(until) {
            sql.push_str(" AND l.timestamp <= ?");
            param_values.push(Box::new(ts));
        }
    }

    sql.push_str(" ORDER BY l.timestamp DESC");
    sql.push_str(&format!(" LIMIT {}", filters.limit));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SQL error: {}", e);
            return vec![];
        }
    };

    stmt.query_map(params_refs.as_slice(), |row| {
        Ok(LogEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            timestamp: row.get(2)?,
            line: row.get(3)?,
            branch: row.get(4)?,
        })
    })
    .unwrap_or_else(|e| {
        eprintln!("Query error: {}", e);
        panic!("Query failed");
    })
    .filter_map(|r| r.ok())
    .collect()
}

pub fn list_sessions(conn: &Connection, filters: &SessionFilters) -> Vec<Session> {
    let mut sql = String::from(
        "SELECT s.id, s.repo, s.branch, s.commit_sha, s.started_at, \
         (SELECT MAX(timestamp) FROM logs WHERE session_id = s.id) as last_log_at, \
         s.cwd, \
         (SELECT COUNT(*) FROM logs WHERE session_id = s.id) as log_count \
         FROM sessions s WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

    if let Some(ref branch) = filters.branch {
        sql.push_str(" AND s.branch = ?");
        param_values.push(Box::new(branch.clone()));
    }
    if let Some(ref commit) = filters.commit {
        sql.push_str(" AND s.commit_sha LIKE ?");
        param_values.push(Box::new(format!("{}%", commit)));
    }
    if let Some(ref since) = filters.since {
        if let Some(ts) = parse_since(since) {
            sql.push_str(" AND s.started_at >= ?");
            param_values.push(Box::new(ts));
        }
    }

    sql.push_str(" ORDER BY s.started_at DESC");
    sql.push_str(&format!(" LIMIT {}", filters.limit));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql).expect("Failed to prepare query");
    stmt.query_map(params_refs.as_slice(), |row| {
        Ok(Session {
            id: row.get(0)?,
            repo: row.get(1)?,
            branch: row.get(2)?,
            commit_sha: row.get(3)?,
            started_at: row.get(4)?,
            last_log_at: row.get(5)?,
            cwd: row.get(6)?,
            log_count: row.get(7)?,
        })
    })
    .expect("Failed to query sessions")
    .filter_map(|r| r.ok())
    .collect()
}

pub fn session_stats(conn: &Connection, session_id: Option<&str>) -> Option<Stats> {
    let sid = if let Some(id) = session_id {
        id.to_string()
    } else {
        // Get latest session
        conn.query_row(
            "SELECT id FROM sessions ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()?
    };

    let session: (Option<String>, Option<String>, Option<String>, String) = conn
        .query_row(
            "SELECT repo, branch, commit_sha, started_at FROM sessions WHERE id = ?1",
            params![sid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok()?;

    let total_lines: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM logs WHERE session_id = ?1",
            params![sid],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let first_log: Option<String> = conn
        .query_row(
            "SELECT timestamp FROM logs WHERE session_id = ?1 ORDER BY timestamp ASC LIMIT 1",
            params![sid],
            |row| row.get(0),
        )
        .ok();

    let last_log: Option<String> = conn
        .query_row(
            "SELECT timestamp FROM logs WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT 1",
            params![sid],
            |row| row.get(0),
        )
        .ok();

    Some(Stats {
        session_id: sid,
        repo: session.0,
        branch: session.1,
        commit_sha: session.2,
        started_at: session.3,
        total_lines,
        first_log,
        last_log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = open_memory_db().unwrap();
        // Create two sessions
        conn.execute(
            "INSERT INTO sessions (id, repo, branch, commit_sha, started_at, cwd) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["sess-1", "org/repo", "main", "abc1234", "2026-03-28T10:00:00+00:00", "/tmp"],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, repo, branch, commit_sha, started_at, cwd) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["sess-2", "org/repo", "feat/login", "def5678", "2026-03-28T12:00:00+00:00", "/tmp"],
        ).unwrap();

        // Insert logs for sess-1
        let logs: Vec<(String, String)> = vec![
            ("2026-03-28T10:00:01+00:00".into(), "server started on port 3000".into()),
            ("2026-03-28T10:00:02+00:00".into(), "GET /api/health 200".into()),
            ("2026-03-28T10:00:03+00:00".into(), "ERROR: connection refused to database".into()),
            ("2026-03-28T10:00:04+00:00".into(), "GET /api/users 200 12ms".into()),
            ("2026-03-28T10:00:05+00:00".into(), "POST /api/users 201 45ms".into()),
        ];
        insert_logs_batch(&conn, "sess-1", &logs).unwrap();

        // Insert logs for sess-2
        let logs2: Vec<(String, String)> = vec![
            ("2026-03-28T12:00:01+00:00".into(), "server started on port 3000".into()),
            ("2026-03-28T12:00:02+00:00".into(), "GET /api/health 200".into()),
            ("2026-03-28T12:00:03+00:00".into(), "connected to database successfully".into()),
        ];
        insert_logs_batch(&conn, "sess-2", &logs2).unwrap();

        conn
    }

    #[test]
    fn test_search_finds_keyword() {
        let conn = setup();
        let results = search_logs(&conn, "ERROR", &SearchFilters {
            branch: None, session_id: None, since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 1);
        assert!(results[0].line.contains("ERROR"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let conn = setup();
        let results = search_logs(&conn, "error", &SearchFilters {
            branch: None, session_id: None, since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 1);
        assert!(results[0].line.contains("ERROR"));
    }

    #[test]
    fn test_search_multiple_matches() {
        let conn = setup();
        let results = search_logs(&conn, "200", &SearchFilters {
            branch: None, session_id: None, since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 3); // GET health + GET users in sess-1, GET health in sess-2
    }

    #[test]
    fn test_search_respects_limit() {
        let conn = setup();
        let results = search_logs(&conn, "200", &SearchFilters {
            branch: None, session_id: None, since: None, until: None, limit: 2,
        });
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_filter_by_session() {
        let conn = setup();
        let results = search_logs(&conn, "200", &SearchFilters {
            branch: None, session_id: Some("sess-1".into()), since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.session_id == "sess-1"));
    }

    #[test]
    fn test_search_filter_by_branch() {
        let conn = setup();
        let results = search_logs(&conn, "server started", &SearchFilters {
            branch: Some("main".into()), session_id: None, since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "sess-1");
    }

    #[test]
    fn test_search_no_results() {
        let conn = setup();
        let results = search_logs(&conn, "nonexistent string xyz", &SearchFilters {
            branch: None, session_id: None, since: None, until: None, limit: 100,
        });
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_search_filter_by_since() {
        let conn = setup();
        let results = search_logs(&conn, "server started", &SearchFilters {
            branch: None, session_id: None,
            since: Some("2026-03-28T11:00:00+00:00".into()),
            until: None, limit: 100,
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "sess-2");
    }

    #[test]
    fn test_search_filter_by_until() {
        let conn = setup();
        let results = search_logs(&conn, "server started", &SearchFilters {
            branch: None, session_id: None, since: None,
            until: Some("2026-03-28T11:00:00+00:00".into()),
            limit: 100,
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "sess-1");
    }

    #[test]
    fn test_list_logs_returns_newest_first() {
        let conn = setup();
        let results = list_logs(&conn, &LogsFilters {
            session_id: Some("sess-1".into()), branch: None,
            since: None, until: None, limit: 50, offset: 0,
        });
        assert_eq!(results.len(), 5);
        assert!(results[0].line.contains("POST")); // newest
        assert!(results[4].line.contains("server started")); // oldest
    }

    #[test]
    fn test_list_logs_pagination() {
        let conn = setup();
        let page1 = list_logs(&conn, &LogsFilters {
            session_id: Some("sess-1".into()), branch: None,
            since: None, until: None, limit: 2, offset: 0,
        });
        let page2 = list_logs(&conn, &LogsFilters {
            session_id: Some("sess-1".into()), branch: None,
            since: None, until: None, limit: 2, offset: 2,
        });
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[test]
    fn test_list_logs_time_range() {
        let conn = setup();
        let results = list_logs(&conn, &LogsFilters {
            session_id: None, branch: None,
            since: Some("2026-03-28T10:00:02+00:00".into()),
            until: Some("2026-03-28T10:00:04+00:00".into()),
            limit: 50, offset: 0,
        });
        assert_eq!(results.len(), 3); // 10:00:02, 10:00:03, 10:00:04
    }

    #[test]
    fn test_list_sessions() {
        let conn = setup();
        let sessions = list_sessions(&conn, &SessionFilters {
            branch: None, commit: None, since: None, limit: 20,
        });
        assert_eq!(sessions.len(), 2);
        // Newest first
        assert_eq!(sessions[0].id, "sess-2");
        assert_eq!(sessions[1].id, "sess-1");
    }

    #[test]
    fn test_list_sessions_filter_by_branch() {
        let conn = setup();
        let sessions = list_sessions(&conn, &SessionFilters {
            branch: Some("feat/login".into()), commit: None, since: None, limit: 20,
        });
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].branch.as_deref(), Some("feat/login"));
    }

    #[test]
    fn test_list_sessions_filter_by_commit_prefix() {
        let conn = setup();
        let sessions = list_sessions(&conn, &SessionFilters {
            branch: None, commit: Some("abc".into()), since: None, limit: 20,
        });
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "sess-1");
    }

    #[test]
    fn test_list_sessions_includes_log_count() {
        let conn = setup();
        let sessions = list_sessions(&conn, &SessionFilters {
            branch: None, commit: None, since: None, limit: 20,
        });
        assert_eq!(sessions[0].log_count, 3); // sess-2
        assert_eq!(sessions[1].log_count, 5); // sess-1
    }

    #[test]
    fn test_list_sessions_includes_last_log_at() {
        let conn = setup();
        let sessions = list_sessions(&conn, &SessionFilters {
            branch: None, commit: None, since: None, limit: 20,
        });
        assert_eq!(sessions[1].last_log_at.as_deref(), Some("2026-03-28T10:00:05+00:00"));
    }

    #[test]
    fn test_session_stats_latest() {
        let conn = setup();
        let stats = session_stats(&conn, None).unwrap();
        assert_eq!(stats.session_id, "sess-2"); // latest
        assert_eq!(stats.total_lines, 3);
        assert_eq!(stats.branch.as_deref(), Some("feat/login"));
        assert_eq!(stats.commit_sha.as_deref(), Some("def5678"));
    }

    #[test]
    fn test_session_stats_specific() {
        let conn = setup();
        let stats = session_stats(&conn, Some("sess-1")).unwrap();
        assert_eq!(stats.total_lines, 5);
        assert_eq!(stats.first_log.as_deref(), Some("2026-03-28T10:00:01+00:00"));
        assert_eq!(stats.last_log.as_deref(), Some("2026-03-28T10:00:05+00:00"));
    }

    #[test]
    fn test_session_stats_nonexistent() {
        let conn = setup();
        assert!(session_stats(&conn, Some("nonexistent")).is_none());
    }

    #[test]
    fn test_parse_since_relative() {
        // Just test that relative times produce a timestamp (can't assert exact value)
        assert!(parse_since("1h").is_some());
        assert!(parse_since("30m").is_some());
        assert!(parse_since("2d").is_some());
        assert!(parse_since("60s").is_some());
    }

    #[test]
    fn test_parse_since_iso8601() {
        let ts = "2026-03-28T10:00:00+00:00";
        assert_eq!(parse_since(ts), Some(ts.to_string()));
    }

    #[test]
    fn test_parse_since_invalid() {
        assert!(parse_since("abc").is_none());
        assert!(parse_since("").is_none());
        assert!(parse_since("1x").is_none());
    }
}

