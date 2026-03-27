mod collector;
mod db;
mod git;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "logbox", about = "Dev log black box — capture and query dev server logs")]
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

    /// Search logs by regex pattern
    Search {
        /// Regex pattern to search for
        pattern: String,

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

        /// Filter to lines matching this regex pattern
        #[arg(long)]
        pattern: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
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

        Commands::Sessions {
            branch,
            since,
            limit,
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            let sessions = db::list_sessions(
                &conn,
                &db::SessionFilters {
                    branch,
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
                    let ended = s.ended_at.as_deref().unwrap_or("(running)");
                    println!(
                        "{} | {} @ {} | {} → {} | {} lines | {}",
                        &s.id[..8],
                        repo_str,
                        branch_str,
                        s.started_at,
                        ended,
                        s.log_count,
                        s.cwd.as_deref().unwrap_or(""),
                    );
                }
            }
        }

        Commands::Search {
            pattern,
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
                        println!("Started:    {}", stats.started_at);
                        println!(
                            "Ended:      {}",
                            stats.ended_at.as_deref().unwrap_or("(running)")
                        );
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
            json,
        } => {
            let conn = db::open_db().expect("Failed to open database");
            match db::compare_sessions(&conn, &session_a, &session_b, pattern.as_deref()) {
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
    }
}
