use std::process::{Command, Stdio};
use std::io::Write;
use std::thread;
use std::time::Duration;

fn logbox_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn collect_lines(home: &std::path::Path, lines: &[&str], quiet: bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_logbox"));
    cmd.env("HOME", home);
    cmd.arg("collect");
    if quiet {
        cmd.arg("--quiet");
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for line in lines {
            writeln!(stdin, "{}", line).unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "collect failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn run_logbox(home: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_logbox"))
        .env("HOME", home)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "logbox {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

fn run_logbox_json(home: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let mut full_args = args.to_vec();
    full_args.push("--json");
    let output = run_logbox(home, &full_args);
    serde_json::from_str(&output).unwrap_or_else(|e| panic!("invalid JSON: {}\noutput: {}", e, output))
}

// --- Collect tests ---

#[test]
fn test_collect_passthrough() {
    let home = logbox_dir();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_logbox"));
    cmd.env("HOME", home.path());
    cmd.args(["collect"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "hello world").unwrap();
        writeln!(stdin, "goodbye world").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello world"));
    assert!(stdout.contains("goodbye world"));
}

#[test]
fn test_collect_quiet_no_stdout() {
    let home = logbox_dir();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_logbox"));
    cmd.env("HOME", home.path());
    cmd.args(["collect", "--quiet"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "should not appear").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_empty(), "quiet mode should produce no stdout, got: {}", stdout);
}

// --- Sessions tests ---

#[test]
fn test_sessions_lists_collections() {
    let home = logbox_dir();
    collect_lines(home.path(), &["line a"], true);
    collect_lines(home.path(), &["line b"], true);

    let sessions = run_logbox_json(home.path(), &["sessions"]);
    let arr = sessions.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
fn test_sessions_include_log_count() {
    let home = logbox_dir();
    collect_lines(home.path(), &["one", "two", "three"], true);

    let sessions = run_logbox_json(home.path(), &["sessions"]);
    let count = sessions[0]["log_count"].as_i64().unwrap();
    assert_eq!(count, 3);
}

// --- Logs tests ---

#[test]
fn test_logs_returns_collected_lines() {
    let home = logbox_dir();
    collect_lines(home.path(), &["alpha", "beta", "gamma"], true);

    let logs = run_logbox_json(home.path(), &["logs"]);
    let arr = logs.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    let lines: Vec<&str> = arr.iter().map(|l| l["line"].as_str().unwrap()).collect();
    assert!(lines.contains(&"alpha"));
    assert!(lines.contains(&"beta"));
    assert!(lines.contains(&"gamma"));
}

#[test]
fn test_logs_newest_first() {
    let home = logbox_dir();
    collect_lines(home.path(), &["first", "second", "third"], true);

    let logs = run_logbox_json(home.path(), &["logs"]);
    let arr = logs.as_array().unwrap();
    // Newest first — "third" was collected last
    assert_eq!(arr[0]["line"].as_str().unwrap(), "third");
    assert_eq!(arr[2]["line"].as_str().unwrap(), "first");
}

#[test]
fn test_logs_limit() {
    let home = logbox_dir();
    collect_lines(home.path(), &["a", "b", "c", "d", "e"], true);

    let logs = run_logbox_json(home.path(), &["logs", "--limit", "2"]);
    assert_eq!(logs.as_array().unwrap().len(), 2);
}

#[test]
fn test_logs_offset() {
    let home = logbox_dir();
    collect_lines(home.path(), &["a", "b", "c", "d", "e"], true);

    let page1 = run_logbox_json(home.path(), &["logs", "--limit", "2", "--offset", "0"]);
    let page2 = run_logbox_json(home.path(), &["logs", "--limit", "2", "--offset", "2"]);

    let id1 = page1[0]["id"].as_i64().unwrap();
    let id2 = page2[0]["id"].as_i64().unwrap();
    assert_ne!(id1, id2);
}

// --- Search tests ---

#[test]
fn test_search_finds_keyword() {
    let home = logbox_dir();
    collect_lines(home.path(), &[
        "GET /api/health 200",
        "ERROR: connection refused",
        "GET /api/users 200",
    ], true);

    let results = run_logbox_json(home.path(), &["search", "ERROR"]);
    let arr = results.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["line"].as_str().unwrap().contains("ERROR"));
}

#[test]
fn test_search_case_insensitive() {
    let home = logbox_dir();
    collect_lines(home.path(), &[
        "ERROR: something broke",
        "everything is fine",
    ], true);

    let results = run_logbox_json(home.path(), &["search", "error"]);
    assert_eq!(results.as_array().unwrap().len(), 1);
}

#[test]
fn test_search_no_results() {
    let home = logbox_dir();
    collect_lines(home.path(), &["hello world"], true);

    let results = run_logbox_json(home.path(), &["search", "zzz_nonexistent"]);
    assert_eq!(results.as_array().unwrap().len(), 0);
}

#[test]
fn test_search_respects_limit() {
    let home = logbox_dir();
    collect_lines(home.path(), &[
        "error one", "error two", "error three", "error four",
    ], true);

    let results = run_logbox_json(home.path(), &["search", "error", "--limit", "2"]);
    assert_eq!(results.as_array().unwrap().len(), 2);
}

// --- Stats tests ---

#[test]
fn test_stats_latest_session() {
    let home = logbox_dir();
    collect_lines(home.path(), &["a", "b"], true);
    collect_lines(home.path(), &["c", "d", "e"], true);

    let stats = run_logbox_json(home.path(), &["stats"]);
    // Latest session has 3 lines
    assert_eq!(stats["total_lines"].as_i64().unwrap(), 3);
}

// --- MCP server tests ---

#[test]
fn test_mcp_initialize() {
    let home = logbox_dir();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_logbox"));
    cmd.env("HOME", home.path());
    cmd.args(["serve"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"capabilities":{{}},"clientInfo":{{"name":"test","version":"0.1.0"}},"protocolVersion":"2024-11-05"}}}}"#).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let response: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(response["result"]["capabilities"]["tools"].is_object());
}

/// Send MCP messages and collect responses. Adds a small delay before closing
/// stdin to ensure the server processes all messages.
fn mcp_exchange(home: &std::path::Path, messages: &[&str]) -> Vec<serde_json::Value> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_logbox"));
    cmd.env("HOME", home);
    cmd.args(["serve"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for msg in messages {
            writeln!(stdin, "{}", msg).unwrap();
            stdin.flush().unwrap();
            thread::sleep(Duration::from_millis(100));
        }
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    stdout
        .trim()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

const INIT_MSG: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"},"protocolVersion":"2024-11-05"}}"#;
const INITIALIZED_MSG: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

#[test]
fn test_mcp_tools_list() {
    let home = logbox_dir();
    let responses = mcp_exchange(home.path(), &[
        INIT_MSG,
        INITIALIZED_MSG,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ]);

    let tools_response = responses.iter().find(|r| r["id"] == 2).expect("no tools/list response");
    let tools = tools_response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"list_logs"));
    assert!(tool_names.contains(&"search_logs"));
    assert!(tool_names.contains(&"list_sessions"));
    assert!(tool_names.contains(&"session_stats"));
}

#[test]
fn test_mcp_search_logs_tool() {
    let home = logbox_dir();
    collect_lines(home.path(), &["GET /health 200", "ERROR: db down", "POST /api 201"], true);

    let responses = mcp_exchange(home.path(), &[
        INIT_MSG,
        INITIALIZED_MSG,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_logs","arguments":{"pattern":"ERROR"}}}"#,
    ]);

    let tool_response = responses.iter().find(|r| r["id"] == 2).expect("no tools/call response");
    let content_text = tool_response["result"]["content"][0]["text"].as_str().unwrap();
    let results: Vec<serde_json::Value> = serde_json::from_str(content_text).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0]["line"].as_str().unwrap().contains("ERROR"));
}
