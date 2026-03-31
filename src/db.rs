use chrono::{DateTime, Duration, Local, Utc};
use regex::Regex;
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

#[derive(Debug, Serialize)]
pub struct CompareResult {
    pub session_a: CompareSessionInfo,
    pub session_b: CompareSessionInfo,
    pub only_in_a: i64,
    pub only_in_b: i64,
    pub common_lines: i64,
}

#[derive(Debug, Serialize)]
pub struct CompareSessionInfo {
    pub id: String,
    pub branch: Option<String>,
    pub total_lines: i64,
}

pub struct SearchFilters {
    pub branch: Option<String>,
    pub session_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: u32,
    pub is_regex: bool,
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
    let re = if filters.is_regex {
        match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Invalid regex pattern: {}", e);
                return vec![];
            }
        }
    } else {
        // Plain text search — escape to literal regex
        Regex::new(&regex::escape(pattern)).unwrap()
    };

    let mut sql = String::from(
        "SELECT l.id, l.session_id, l.timestamp, l.line, s.branch \
         FROM logs l JOIN sessions s ON l.session_id = s.id WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

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
    // Fetch more than limit since we filter by regex in Rust
    sql.push_str(&format!(" LIMIT {}", filters.limit * 10));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SQL error: {}", e);
            return vec![];
        }
    };

    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
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
        });

    let mut results = vec![];
    for row in rows {
        if let Ok(entry) = row {
            if re.is_match(&entry.line) {
                results.push(entry);
                if results.len() >= filters.limit as usize {
                    break;
                }
            }
        }
    }
    results
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

pub fn compare_sessions(
    conn: &Connection,
    session_a: &str,
    session_b: &str,
    pattern: Option<&str>,
    is_regex: bool,
) -> Option<CompareResult> {
    let re = pattern.map(|p| {
        if is_regex {
            Regex::new(p).unwrap_or_else(|_| Regex::new(&regex::escape(p)).unwrap())
        } else {
            Regex::new(&regex::escape(p)).unwrap()
        }
    });

    let get_info = |sid: &str| -> Option<CompareSessionInfo> {
        let (branch, total): (Option<String>, i64) = conn
            .query_row(
                "SELECT s.branch, COUNT(l.id) FROM sessions s \
                 LEFT JOIN logs l ON l.session_id = s.id \
                 WHERE s.id = ?1 GROUP BY s.id",
                params![sid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()?;
        Some(CompareSessionInfo {
            id: sid.to_string(),
            branch,
            total_lines: total,
        })
    };

    let info_a = get_info(session_a)?;
    let info_b = get_info(session_b)?;

    // Get lines from each session
    let get_lines = |sid: &str| -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT line FROM logs WHERE session_id = ?1 ORDER BY timestamp")
            .unwrap();
        stmt.query_map(params![sid], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .filter(|line| re.as_ref().map_or(true, |r| r.is_match(line)))
            .collect()
    };

    let lines_a: std::collections::HashSet<String> = get_lines(session_a).into_iter().collect();
    let lines_b: std::collections::HashSet<String> = get_lines(session_b).into_iter().collect();

    let common = lines_a.intersection(&lines_b).count() as i64;
    let only_a = lines_a.len() as i64 - common;
    let only_b = lines_b.len() as i64 - common;

    Some(CompareResult {
        session_a: info_a,
        session_b: info_b,
        only_in_a: only_a,
        only_in_b: only_b,
        common_lines: common,
    })
}
