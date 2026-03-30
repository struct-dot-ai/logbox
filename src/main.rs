mod collector;
mod db;
mod git;
mod server;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "logbox", about = "Persist dev server logs to SQLite so AI coding agents can search and query them. Pipe your dev server through `logbox collect`, then agents can use search/sessions/stats/compare to investigate issues.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Collect logs from stdin into the database
    Collect {
        /// Working directory to associate with this session
        #[arg(long)]
        cwd: Option<String>,

        /// Don't echo logs to stdout (silent mode)
        #[arg(short, long)]
        quiet: bool,
    },

    /// List recorded sessions
    Sessions {
        /// Filter by git branch
        #[arg(long)]
        branch: Option<String>,

        /// Filter by commit SHA (prefix match)
        #[arg(long)]
        commit: Option<String>,

        /// Only sessions started after this time (e.g. "1h", "30m", "2d", or ISO 8601)
        #[arg(long)]
        since: Option<String>,

        /// Maximum number of sessions to show
        #[arg(long, default_value = "20")]
        limit: u32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show recent logs (newest first, paginate with --offset)
    Logs {
        /// Filter to a specific session ID
        #[arg(long)]
        session: Option<String>,

        /// Filter by git branch
        #[arg(long)]
        branch: Option<String>,

        /// Only logs after this time (e.g. "1h", "30m", "2d")
        #[arg(long)]
        since: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: u32,

        /// Skip this many results (for pagination)
        #[arg(long, default_value = "0")]
        offset: u32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Search logs by keyword or pattern
    Search {
        /// Text to search for (plain text by default, use --regex for regex)
        pattern: String,

        /// Treat pattern as a regular expression
        #[arg(long)]
        regex: bool,

        /// Filter to logs from the last duration (e.g. "1h", "30m", "2d")
        #[arg(long)]
        last: Option<String>,

        /// Filter to a specific session ID
        #[arg(long)]
        session: Option<String>,

        /// Filter by git branch
        #[arg(long)]
        branch: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "100")]
        limit: u32,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show stats for a session
    Stats {
        /// Session ID (defaults to latest session)
        session_id: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compare logs between two sessions
    Compare {
        /// First session ID
        session_a: String,

        /// Second session ID
        session_b: String,

        /// Filter to lines matching this pattern
        #[arg(long)]
        pattern: Option<String>,

        /// Treat pattern as a regular expression
        #[arg(long)]
        regex: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Start MCP server for AI coding agents
    Serve,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect { cwd, quiet } => {
            if let Err(e) = collector::start(quiet, cwd) {
                eprintln!("logbox: collector error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Logs {
            session,
            branch,
            since,
            limit,
            offset,
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            let results = db::list_logs(
                &conn,
                &db::LogsFilters {
                    session_id: session,
                    branch,
                    since,
                    limit,
                    offset,
                },
            );

            if json {
                println!("{}", serde_json::to_string_pretty(&results).unwrap());
            } else if results.is_empty() {
                println!("No logs found.");
            } else {
                for entry in &results {
                    let branch_str = entry
                        .branch
                        .as_deref()
                        .map(|b| format!(" [{}]", b))
                        .unwrap_or_default();
                    println!(
                        "{} {}{}  {}",
                        &entry.session_id[..8],
                        entry.timestamp,
                        branch_str,
                        entry.line,
                    );
                }
            }
        }

        Commands::Sessions {
            branch,
            commit,
            since,
            limit,
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            let sessions = db::list_sessions(
                &conn,
                &db::SessionFilters {
                    branch,
                    commit,
                    since,
                    limit,
                },
            );

            if json {
                println!("{}", serde_json::to_string_pretty(&sessions).unwrap());
            } else if sessions.is_empty() {
                println!("No sessions found.");
            } else {
                for s in &sessions {
                    let repo_str = s.repo.as_deref().unwrap_or("(no repo)");
                    let branch_str = s.branch.as_deref().unwrap_or("(no branch)");
                    let sha_short = s.commit_sha.as_deref().map(|s| &s[..7]).unwrap_or("-------");
                    let last_log = s.last_log_at.as_deref().unwrap_or("(no logs)");
                    println!(
                        "{} | {} @ {} ({}) | started {} | last log {} | {} lines",
                        &s.id[..8],
                        repo_str,
                        branch_str,
                        sha_short,
                        s.started_at,
                        last_log,
                        s.log_count,
                    );
                }
            }
        }

        Commands::Search {
            pattern,
            regex,
            last,
            session,
            branch,
            limit,
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            let results = db::search_logs(
                &conn,
                &pattern,
                &db::SearchFilters {
                    branch,
                    session_id: session,
                    since: last,
                    until: None,
                    limit,
                    is_regex: regex,
                },
            );

            if json {
                println!("{}", serde_json::to_string_pretty(&results).unwrap());
            } else if results.is_empty() {
                println!("No matching logs found.");
            } else {
                for entry in &results {
                    let branch_str = entry
                        .branch
                        .as_deref()
                        .map(|b| format!(" [{}]", b))
                        .unwrap_or_default();
                    println!(
                        "{} {}{}  {}",
                        &entry.session_id[..8],
                        entry.timestamp,
                        branch_str,
                        entry.line,
                    );
                }
            }
        }

        Commands::Stats { session_id, json } => {
            let conn = db::open_db().expect("Failed to open database");
            match db::session_stats(&conn, session_id.as_deref()) {
                Some(stats) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
                    } else {
                        println!("Session:    {}", stats.session_id);
                        println!(
                            "Repo:       {}",
                            stats.repo.as_deref().unwrap_or("(no repo)")
                        );
                        println!(
                            "Branch:     {}",
                            stats.branch.as_deref().unwrap_or("(no branch)")
                        );
                        println!(
                            "Commit:     {}",
                            stats.commit_sha.as_deref().unwrap_or("(unknown)")
                        );
                        println!("Started:    {}", stats.started_at);
                        println!("Lines:      {}", stats.total_lines);
                        if let Some(ref first) = stats.first_log {
                            println!("First log:  {}", first);
                        }
                        if let Some(ref last) = stats.last_log {
                            println!("Last log:   {}", last);
                        }
                    }
                }
                None => {
                    eprintln!("No session found.");
                    std::process::exit(1);
                }
            }
        }

        Commands::Compare {
            session_a,
            session_b,
            pattern,
            regex,
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            match db::compare_sessions(&conn, &session_a, &session_b, pattern.as_deref(), regex) {
                Some(result) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    } else {
                        println!(
                            "Session A: {} ({} lines, branch: {})",
                            &result.session_a.id[..8],
                            result.session_a.total_lines,
                            result.session_a.branch.as_deref().unwrap_or("none"),
                        );
                        println!(
                            "Session B: {} ({} lines, branch: {})",
                            &result.session_b.id[..8],
                            result.session_b.total_lines,
                            result.session_b.branch.as_deref().unwrap_or("none"),
                        );
                        println!();
                        println!("Only in A:    {}", result.only_in_a);
                        println!("Only in B:    {}", result.only_in_b);
                        println!("Common lines: {}", result.common_lines);
                    }
                }
                None => {
                    eprintln!("One or both sessions not found.");
                    std::process::exit(1);
                }
            }
        }

        Commands::Serve => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            if let Err(e) = rt.block_on(server::run_server()) {
                eprintln!("logbox: server error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
